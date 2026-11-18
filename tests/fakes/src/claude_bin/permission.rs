//! The `permission` script step: a permission dialog, as Claude Code 2.1.327
//! draws and answers it. Claude Code runs each `PermissionRequest` hook when
//! the dialog opens, takes the first hook decision, and with none denies the
//! call when its countdown ends. A key typed into the dialog answers it too,
//! so a test can prove that nothing pressed one.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use super::settings::run_shell;
use super::{Event, Input, Polled, Session, say};
use crate::fsutil;
use crate::owner::Owner;

/// Claude Code's own time limit for a dialog with a countdown.
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// How one dialog ended.
struct Outcome {
    /// `hook`, `key` or `timeout`.
    by: &'static str,
    allow: bool,
    message: Option<String>,
}

/// The dashed rule Claude Code draws around the command, 150 columns wide.
fn rule() -> String {
    "\u{254c}".repeat(150)
}

/// The dialog's lines, from its `Bash command` line to its `Esc to cancel`
/// line, as captured from a critical-path removal with its countdown line.
fn dialog_lines(tool: &str, input: &Value) -> Vec<String> {
    let text = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or_default();
    let (title, body) = if tool == "Bash" {
        ("Bash command".to_owned(), text("command").to_owned())
    } else {
        (tool.to_owned(), input.to_string())
    };
    let mut lines = vec![format!(" {title}"), format!(" {}", text("description"))];
    lines.push(rule());
    lines.extend(body.lines().map(|l| format!(" {l}")));
    lines.push(rule());
    if tool == "Bash" {
        lines.push(format!(
            " Dangerous rmdir operation on critical path: {body}"
        ));
    }
    lines.push(
        " \u{26a0} Claude Code will automatically block this request in 1:46, to avoid holding up an unattended session"
            .to_owned(),
    );
    lines.extend(
        [
            "",
            "",
            " Do you want to proceed?",
            " \u{276f} 1. Yes",
            "   2. No",
            "",
            " Esc to cancel \u{b7} Tab to amend",
        ]
        .map(str::to_owned),
    );
    lines
}

/// The decision a hook printed, as Claude Code reads
/// `hookSpecificOutput.decision`. `None` when it decided nothing.
fn hook_decision(stdout: &str) -> Option<(bool, Option<String>)> {
    let doc: Value = serde_json::from_str(stdout.trim()).ok()?;
    let decision = doc.pointer("/hookSpecificOutput/decision")?;
    let allow = match decision.get("behavior")?.as_str()? {
        "allow" => true,
        "deny" => false,
        _ => return None,
    };
    let message = decision
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some((allow, message))
}

impl Session {
    /// The `permission` step. Blocks the agent at the dialog until a hook
    /// decides, a key answers, or `timeout_ms` passes.
    pub(super) fn permission(
        &mut self,
        tool: &str,
        tool_input: &Value,
        timeout_ms: Option<u64>,
        input: &mut Input,
    ) {
        for line in dialog_lines(tool, tool_input) {
            say(&line);
        }
        self.set_status("blocked");
        let hooks = self.spawn_hooks(tool, tool_input);
        let limit = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
        let clock = fsutil::Stopwatch::start();
        let mut hooks_done = false;
        let outcome = loop {
            if !hooks_done {
                match hooks.try_recv() {
                    Ok(Some((allow, message))) => {
                        break Outcome {
                            by: "hook",
                            allow,
                            message,
                        };
                    }
                    Ok(None) | Err(mpsc::TryRecvError::Disconnected) => hooks_done = true,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            match input.poll(true) {
                Polled::Got(Event::Key(key)) => match key.as_str() {
                    "1" | "enter" => {
                        break Outcome {
                            by: "key",
                            allow: true,
                            message: None,
                        };
                    }
                    "2" | "esc" | "ctrl+c" => {
                        break Outcome {
                            by: "key",
                            allow: false,
                            message: None,
                        };
                    }
                    _ => {}
                },
                Polled::Got(Event::Prompt(_)) | Polled::Nothing => {}
                // Input ended: the session is going away, nothing answers.
                Polled::Ended => {
                    break Outcome {
                        by: "timeout",
                        allow: false,
                        message: None,
                    };
                }
            }
            if clock.elapsed() >= limit || self.owner.gone() {
                break Outcome {
                    by: "timeout",
                    allow: false,
                    message: None,
                };
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        self.record_permission(&outcome);
        match (&outcome.by, outcome.allow) {
            (&"hook", true) => say("Allowed by PermissionRequest hook"),
            (&"hook", false) => {
                say("Denied by PermissionRequest hook");
                say(&format!(
                    "Reason: {}",
                    outcome.message.as_deref().unwrap_or_default()
                ));
            }
            (_, true) => say("Allowed"),
            (_, false) => say("Not run \u{b7} permission denied"),
        }
        self.set_status("working");
    }

    /// Runs every `PermissionRequest` hook on a thread with the stdin
    /// Claude Code sends, and sends the first decision, or `None` once every
    /// hook ran without one.
    fn spawn_hooks(
        &self,
        tool: &str,
        tool_input: &Value,
    ) -> mpsc::Receiver<Option<(bool, Option<String>)>> {
        let stdin = json!({
            "session_id": self.id,
            "transcript_path": self.transcript.display().to_string(),
            "cwd": self.cwd,
            "permission_mode": "bypassPermissions",
            "hook_event_name": "PermissionRequest",
            "tool_name": tool,
            "tool_input": tool_input,
            "permission_suggestions": [],
        })
        .to_string();
        let commands = self.settings.permission_hooks.clone();
        let env = self.settings.env.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut owner = Owner::from_env();
            for command in &commands {
                let ran = run_shell(command, &env, Some(&stdin), &mut owner, None);
                if let Some(decision) = hook_decision(&ran.stdout) {
                    tx.send(Some(decision)).ok();
                    return;
                }
            }
            tx.send(None).ok();
        });
        rx
    }

    /// Writes `<claude dir>/permission/<session>-<n>.json`, n counting this
    /// session's dialogs from 1.
    fn record_permission(&self, outcome: &Outcome) {
        let Some(dir) = self
            .file
            .parent()
            .and_then(std::path::Path::parent)
            .map(|d| d.join("permission"))
        else {
            return;
        };
        std::fs::create_dir_all(&dir).ok();
        let prefix = format!("{}-", self.id);
        let n = std::fs::read_dir(&dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                    .count()
            })
            .unwrap_or(0)
            + 1;
        let path: PathBuf = dir.join(format!("{prefix}{n}.json"));
        let doc = json!({
            "by": outcome.by,
            "decision": if outcome.allow { "allow" } else { "deny" },
            "message": outcome.message,
        });
        fsutil::write_atomic(&path, doc.to_string().as_bytes()).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_decision_is_read_from_hook_specific_output() {
        let allow = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;
        assert_eq!(hook_decision(allow), Some((true, None)));
        let deny = r#"{"hookSpecificOutput":{"decision":{"behavior":"deny","message":"no"}}}"#;
        assert_eq!(hook_decision(deny), Some((false, Some("no".to_owned()))));
        assert_eq!(hook_decision(""), None);
        assert_eq!(hook_decision(r#"{"decision":"block"}"#), None);
    }
}
