//! The `--settings` file: its `env` key and its Stop hooks, applied the way
//! Claude Code applies them.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

use crate::fsutil;
use crate::owner::Owner;

#[derive(Debug, Default)]
pub(crate) struct Settings {
    /// Set on every shell step and every hook.
    pub(crate) env: Vec<(String, String)>,
    /// The commands of the `Stop` hooks, in file order.
    pub(crate) stop_hooks: Vec<String>,
    /// The commands of the `PermissionRequest` hooks, in file order.
    pub(crate) permission_hooks: Vec<String>,
}

impl Settings {
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let text = fsutil::read_text(path)
            .ok_or_else(|| format!("cannot read settings {}", path.display()))?;
        let doc: Value = serde_json::from_str(&text)
            .map_err(|e| format!("bad settings {}: {e}", path.display()))?;
        let env = doc
            .get("env")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            env,
            stop_hooks: hook_commands(&doc, "Stop"),
            permission_hooks: hook_commands(&doc, "PermissionRequest"),
        })
    }
}

/// The commands of the `event` hooks, in file order.
fn hook_commands(doc: &Value, event: &str) -> Vec<String> {
    doc.pointer(&format!("/hooks/{event}"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .filter_map(|h| h.get("command").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

/// What a finished shell command left.
#[derive(Debug)]
pub(crate) struct Ran {
    pub(crate) code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// What the shell runs first: it waits for one line on its standard input,
/// the go-ahead, then becomes `sh -c <command>` in the same process, so `$$`
/// stays the group's id. If fake-claude dies before it sends the go-ahead,
/// the pipe closes, `read` fails, and the command never runs.
const GATE: &str = r#"IFS= read -r _ || exit 125; exec /bin/sh -c "$1""#;

/// `sh -c command` behind `GATE`, in a process group of its own.
#[expect(
    clippy::disallowed_methods,
    reason = "fake-claude runs the shell steps and hooks of its script; this crate may start processes"
)]
fn gated_shell(command: &str, env: &[(String, String)]) -> std::process::Command {
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.args(["-c", GATE, "sh", command])
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    cmd
}

/// Runs `sh -c command` with the settings' `env` on top of this process's
/// environment, `stdin` on its standard input, and waits. The command runs
/// in a process group of its own, and `step_file` (if any) holds the group's
/// id while it runs, so whoever ends the agent can end the group too. The
/// id is on record before the command starts (`GATE`): an agent ended at any
/// moment leaves no step running that nobody can find. When `owner` is gone
/// the group is ended and the call returns exit code -1, so a command that
/// never returns cannot outlive the test that started it.
pub(crate) fn run_shell(
    command: &str,
    env: &[(String, String)],
    stdin: Option<&str>,
    owner: &mut Owner,
    step_file: Option<&Path>,
) -> Ran {
    let mut child = match gated_shell(command, env).spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ran {
                code: 127,
                stdout: String::new(),
                stderr: e.to_string(),
            };
        }
    };
    if let Some(file) = step_file {
        fsutil::write_atomic(file, child.id().to_string().as_bytes()).ok();
    }
    if let Some(mut pipe) = child.stdin.take() {
        // The go-ahead, then the input. Dropping the pipe ends the input, as
        // /dev/null did. The child may exit without reading; a broken pipe
        // is fine.
        pipe.write_all(b"\n").ok();
        if let Some(text) = stdin {
            pipe.write_all(text.as_bytes()).ok();
        }
    }
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) => {}
            Err(e) => {
                return Ran {
                    code: 126,
                    stdout: String::new(),
                    stderr: e.to_string(),
                };
            }
        }
        if owner.gone() {
            fsutil::terminate(child.id());
            child.kill().ok();
            child.wait().ok();
            break -1;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // A process the shell left in the background keeps the pipes open, so
    // the readers end only when it does. Wait for it as `wait_with_output`
    // would, but not past the owner: end the group, then give the readers a
    // moment to see the pipes close.
    let busy = || {
        [&stdout, &stderr]
            .iter()
            .any(|r| r.as_ref().is_some_and(|r| !r.is_finished()))
    };
    let mut ended_at = None;
    while busy() {
        if ended_at.is_none() && owner.gone() {
            fsutil::terminate(child.id());
            ended_at = Some(fsutil::Stopwatch::start());
        }
        if ended_at
            .as_ref()
            .is_some_and(|at| at.elapsed() > Duration::from_secs(2))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(file) = step_file {
        std::fs::remove_file(file).ok();
    }
    let text = |reader: Option<std::thread::JoinHandle<Vec<u8>>>| {
        let bytes = reader
            .filter(std::thread::JoinHandle::is_finished)
            .and_then(|r| r.join().ok())
            .unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    Ran {
        code,
        stdout: text(stdout),
        stderr: text(stderr),
    }
}

/// Reads a pipe to its end on a thread, so a full pipe never blocks the
/// child while the caller watches for its exit and its owner.
fn drain(mut pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).ok();
        bytes
    })
}

/// The reason a Stop hook holds the agent, if it does: exit code 2 with the
/// reason on stderr, JSON `{"decision":"block","reason":...}` on stdout, or
/// JSON with `hookSpecificOutput.additionalContext`.
pub(crate) fn hold_reason(ran: &Ran) -> Option<String> {
    if ran.code == 2 {
        return Some(ran.stderr.trim().to_owned());
    }
    let doc: Value = serde_json::from_str(ran.stdout.trim()).ok()?;
    if doc.get("decision").and_then(Value::as_str) == Some("block") {
        return Some(
            doc.get("reason")
                .and_then(Value::as_str)
                .unwrap_or("held by a Stop hook")
                .to_owned(),
        );
    }
    // `hookSpecificOutput.additionalContext` also keeps the agent going.
    doc.pointer("/hookSpecificOutput/additionalContext")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_env_and_stop_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fsutil::write_atomic(
            &path,
            br#"{"env":{"A":"1","N":5},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"},{"type":"command","command":"y"}]}],"PostToolUse":[{"hooks":[{"command":"z"}]}],"PermissionRequest":[{"hooks":[{"command":"p","timeout":110}]}]}}"#,
        )
        .unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.env, [("A".to_owned(), "1".to_owned())]);
        assert_eq!(s.stop_hooks, ["x", "y"]);
        assert_eq!(s.permission_hooks, ["p"]);
    }

    #[test]
    fn shell_steps_see_the_settings_env_and_stdin() {
        let env = vec![("FROM_SETTINGS".to_owned(), "yes".to_owned())];
        let ran = run_shell(
            "printf '%s:' \"$FROM_SETTINGS\"; cat",
            &env,
            Some("in"),
            &mut Owner::from_env(),
            None,
        );
        assert_eq!((ran.code, ran.stdout.as_str()), (0, "yes:in"));
    }

    /// fake-claude dying before the go-ahead closes the pipe, as dropping it
    /// does here: the shell exits and its command never runs.
    #[test]
    fn a_shell_never_runs_its_command_without_the_go_ahead() {
        let dir = tempfile::tempdir().unwrap();
        let ran = dir.path().join("ran");
        let mut child = gated_shell(&format!("touch '{}'", ran.display()), &[])
            .spawn()
            .unwrap();
        drop(child.stdin.take());
        assert_eq!(child.wait().unwrap().code(), Some(125));
        assert!(!ran.exists(), "the command ran without the go-ahead");
    }

    /// Whoever ends the agent finds the step's group in the step file, so the
    /// file must name the group before the command can do anything.
    #[test]
    fn the_step_file_names_the_group_before_the_command_runs() {
        let dir = tempfile::tempdir().unwrap();
        let step_file = dir.path().join("step.pgid");
        let env = vec![("STEP".to_owned(), step_file.display().to_string())];
        let ran = run_shell(
            r#"[ "$(cat "$STEP")" = "$$" ] && echo recorded"#,
            &env,
            None,
            &mut Owner::from_env(),
            Some(&step_file),
        );
        assert_eq!((ran.code, ran.stdout.as_str()), (0, "recorded\n"));
        assert!(!step_file.exists(), "the step file outlived its step");
    }

    #[test]
    fn a_hold_is_json_block_or_exit_2() {
        let json = Ran {
            code: 0,
            stdout: r#"{"decision":"block","reason":"add a progress line"}"#.into(),
            stderr: String::new(),
        };
        assert_eq!(hold_reason(&json).as_deref(), Some("add a progress line"));
        let two = Ran {
            code: 2,
            stdout: String::new(),
            stderr: "held\n".into(),
        };
        assert_eq!(hold_reason(&two).as_deref(), Some("held"));
        let fine = Ran {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert_eq!(hold_reason(&fine), None);
    }
}
