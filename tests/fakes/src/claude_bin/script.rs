//! The script file (`FAKE_CLAUDE_SCRIPT`) that says what `fake-claude` does.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fsutil;

/// One thing `fake-claude` does during a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Run `sh -c <command>` with the settings file's `env` applied, as
    /// Claude Code's Bash tool does. Its output goes to the screen.
    Run(String),
    /// Print a line to the screen.
    Print(String),
    /// Wait this many milliseconds.
    SleepMs(u64),
    /// Leave these background tasks running: the Stop hook input lists them
    /// in `background_tasks`, and a non-empty list means the session is
    /// paused, not finished. Cleared at the next prompt.
    BackgroundTasks(Vec<String>),
    /// Ask permission for one call, as Claude Code does: draw the dialog,
    /// run each `PermissionRequest` hook from `--settings`, and act on its
    /// decision. A key typed while it waits answers the dialog instead, and
    /// with no answer it denies after `timeout_ms` (default 120000). The
    /// outcome is written to `<claude dir>/permission/<session>-<n>.json`.
    Permission {
        tool: String,
        input: serde_json::Value,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
}

/// What happens at the start, before the first prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Startup {
    /// Straight to the input prompt.
    #[default]
    None,
    /// The plain folder-trust dialog.
    Trust,
    /// The trust dialog that also asks to run the folder's hooks and MCP
    /// servers. An adapter must never answer it.
    EscalatedTrust,
}

/// How the end of a turn goes wrong.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    #[default]
    None,
    /// The turn's steps run and the Stop hook never does; the agent stays
    /// `working`.
    NeverStop,
    /// The Stop hook runs twice for one turn.
    StopTwice,
    /// The hook input names a session that is not this one.
    WrongSession,
    /// The process exits after the steps without running the Stop hook.
    ExitWithoutStop,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// Steps for the first prompt, the second, and so on. The last list
    /// repeats for every later prompt. With no lists, a turn does nothing but
    /// stop.
    #[serde(default)]
    pub turns: Vec<Vec<Step>>,
    /// Steps to run when a Stop hook holds the agent, before it stops again.
    #[serde(default)]
    pub on_hold: Vec<Step>,
    #[serde(default)]
    pub startup: Startup,
    #[serde(default)]
    pub fault: Fault,
    /// Never run any hook: a harness without a turn-end hook.
    #[serde(default)]
    pub no_hook: bool,
    /// The text the Stop hook input carries as `last_assistant_message`.
    #[serde(default)]
    pub final_message: Option<String>,
}

impl Script {
    /// The steps for the `n`th prompt (counting from 0).
    pub(crate) fn turn(&self, n: usize) -> &[Step] {
        self.turns
            .get(n)
            .or_else(|| self.turns.last())
            .map_or(&[], Vec::as_slice)
    }
}

/// Reads the script `FAKE_CLAUDE_SCRIPT` names; an unset or unreadable file
/// gives the default script.
pub(crate) fn load(path: Option<&Path>) -> Result<Script, String> {
    let Some(path) = path else {
        return Ok(Script::default());
    };
    let text =
        fsutil::read_text(path).ok_or_else(|| format!("cannot read script {}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("bad script {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_turn_repeats() {
        let script = Script {
            turns: vec![
                vec![Step::Print("one".into())],
                vec![Step::Print("two".into())],
            ],
            ..Script::default()
        };
        assert_eq!(script.turn(0), [Step::Print("one".into())]);
        assert_eq!(script.turn(5), [Step::Print("two".into())]);
        assert!(Script::default().turn(0).is_empty());
    }

    #[test]
    fn a_script_reads_from_json_with_defaults() {
        let s: Script = serde_json::from_str(
            r#"{"turns":[[{"run":"echo hi"},{"sleep_ms":5}]],"fault":"stop_twice","startup":"escalated_trust"}"#,
        )
        .unwrap();
        assert_eq!(s.fault, Fault::StopTwice);
        assert_eq!(s.startup, Startup::EscalatedTrust);
        assert_eq!(s.turn(0).len(), 2);
        assert!(!s.no_hook);
    }
}
