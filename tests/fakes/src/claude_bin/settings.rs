//! The `--settings` file: its `env` key and its Stop hooks, applied the way
//! Claude Code applies them.

use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

use serde_json::Value;

use crate::fsutil;

#[derive(Debug, Default)]
pub(crate) struct Settings {
    /// Set on every shell step and every hook.
    pub(crate) env: Vec<(String, String)>,
    /// The commands of the `Stop` hooks, in file order.
    pub(crate) stop_hooks: Vec<String>,
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
        let stop_hooks = doc
            .pointer("/hooks/Stop")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|group| group.get("hooks").and_then(Value::as_array))
            .flatten()
            .filter_map(|h| h.get("command").and_then(Value::as_str).map(str::to_owned))
            .collect();
        Ok(Self { env, stop_hooks })
    }
}

/// What a finished shell command left.
#[derive(Debug)]
pub(crate) struct Ran {
    pub(crate) code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Runs `sh -c command` with the settings' `env` on top of this process's
/// environment, `stdin` on its standard input, and waits.
#[expect(
    clippy::disallowed_methods,
    reason = "fake-claude runs the shell steps and hooks of its script; this crate may start processes"
)]
pub(crate) fn run_shell(command: &str, env: &[(String, String)], stdin: Option<&str>) -> Ran {
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(command)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ran {
                code: 127,
                stdout: String::new(),
                stderr: e.to_string(),
            };
        }
    };
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // The child may exit without reading; a broken pipe is fine.
        pipe.write_all(text.as_bytes()).ok();
    }
    match child.wait_with_output() {
        Ok(out) => Ran {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => Ran {
            code: 126,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    }
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
            br#"{"env":{"A":"1","N":5},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"},{"type":"command","command":"y"}]}],"PostToolUse":[{"hooks":[{"command":"z"}]}]}}"#,
        )
        .unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.env, [("A".to_owned(), "1".to_owned())]);
        assert_eq!(s.stop_hooks, ["x", "y"]);
    }

    #[test]
    fn shell_steps_see_the_settings_env_and_stdin() {
        let env = vec![("FROM_SETTINGS".to_owned(), "yes".to_owned())];
        let ran = run_shell("printf '%s:' \"$FROM_SETTINGS\"; cat", &env, Some("in"));
        assert_eq!((ran.code, ran.stdout.as_str()), (0, "yes:in"));
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
