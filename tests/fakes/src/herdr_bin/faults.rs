//! The fault file (`FAKE_HERDR_FAULTS`): rules that make chosen calls fail,
//! hang, print garbage or stall. Rules are matched in file order.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::state::State;
use crate::fsutil;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    #[serde(default)]
    pub message: String,
}

/// What a matching rule does to the call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Print herdr's error envelope on stderr and exit 1. Nothing changes.
    Error(ErrorBody),
    /// Sleep for `FAKE_HERDR_HANG_MS` (default 30 s) before doing anything.
    Hang,
    /// Print text that is not JSON on stdout and exit 0.
    Garbage,
    /// `agent prompt --wait` only: type the text into the agent, then fail
    /// with `agent_prompt_stalled`, as herdr does when it saw no activity.
    /// Without `--wait` the call works as usual, because herdr never reports
    /// a stall then, and every other command ignores the rule. The firing
    /// still counts.
    Stalled,
    /// `agent prompt` only: type the text into the agent, then fail with
    /// `timeout`, as a call does that ran out of time. Other commands ignore
    /// the rule. The firing still counts.
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// The command words, such as `"agent start"`. Absent matches any call.
    #[serde(default)]
    pub cmd: Option<String>,
    /// The first argument after the command (an agent name, a pane id).
    /// Absent matches any target.
    #[serde(default)]
    pub target: Option<String>,
    /// How many times the rule fires before it stops. Absent means always.
    #[serde(default)]
    pub times: Option<u32>,
    pub action: Action,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct FaultFile {
    #[serde(default)]
    pub(crate) rules: Vec<Rule>,
}

pub(crate) fn load(path: &Path) -> FaultFile {
    fsutil::read_text(path)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The action of the first rule that matches and has firings left. Counts
/// the firing in `state`.
pub(crate) fn take(
    file: &FaultFile,
    state: &mut State,
    cmd: &str,
    target: Option<&str>,
) -> Option<Action> {
    for (index, rule) in file.rules.iter().enumerate() {
        let cmd_ok = rule.cmd.as_deref().is_none_or(|c| c == cmd);
        let target_ok = rule.target.as_deref().is_none_or(|t| Some(t) == target);
        if !(cmd_ok && target_ok) {
            continue;
        }
        let hits = state.fault_hits.entry(index.to_string()).or_insert(0);
        if rule.times.is_some_and(|limit| *hits >= limit) {
            continue;
        }
        *hits += 1;
        return Some(rule.action.clone());
    }
    None
}
