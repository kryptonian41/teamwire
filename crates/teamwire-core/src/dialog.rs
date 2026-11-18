//! Permission dialogs a member's harness shows, and the lead's answer to one
//! (ADR 0052). Never: a type here that names a lasting grant, or a key.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::names::{SessionId, Ulid};
use crate::sanitize::is_invisible;
use crate::strings::string_enum;
use crate::timestamp::Timestamp;

string_enum! {
    /// What a member's screen shows, as its harness reads it.
    pub enum DialogKind {
        NoDialog => "none",
        Permission => "permission",
        Trust => "trust",
        Login => "login",
        Update => "update",
        Unknown => "unknown",
        /// The harness cannot read its own dialogs.
        CannotTell => "cannot_tell",
    }
}

string_enum! {
    /// The lead's verdict on one permission dialog.
    pub enum DialogVerdict { Approve => "approve", Deny => "deny" }
}

string_enum! {
    /// Read from the events, never stored as a column.
    pub enum DialogState {
        Pending => "pending",
        /// The lead answered and the hook has not taken it yet.
        Answered => "answered",
        Taken => "taken",
        Expired => "expired",
        Unanswerable => "unanswerable",
    }
}

/// One call a member's harness asks permission for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub session: SessionId,
    /// The call came from a subagent inside the member's session.
    pub subagent: bool,
    pub tool: String,
    /// One short text for the watch line: the command, the path or the URL.
    /// At most `SUMMARY_MAX` characters.
    pub summary: String,
    /// The whole tool input. `None` when it is over `PERMISSION_INPUT_MAX`.
    pub input: Option<serde_json::Value>,
    /// Why teamwire will not answer this call. `None` means the lead may.
    pub why_not: Option<String>,
}

impl PermissionRequest {
    pub fn answerable(&self) -> bool {
        self.why_not.is_none()
    }
}

/// The lead's answer, as `dialog answer` records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionAnswer {
    pub verdict: DialogVerdict,
    pub reason: String,
}

/// What the hook hands the member's harness to print. A denial's text
/// travels only here, inside the member's own tool result, so nothing types
/// into the member's pane or sends it a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDecision {
    pub verdict: DialogVerdict,
    /// For a denial, the whole text the member's model reads. None for an approval.
    pub message: Option<String>,
}

/// How long the hook waits for the lead. Claude Code denies at 120 s.
pub const PERMISSION_WAIT: Duration = Duration::from_secs(100);
/// The hook's `timeout` in the member settings: above the wait, below 120 s.
pub const PERMISSION_HOOK_TIMEOUT_S: u64 = 110;
/// How long `dialog answer` waits for the hook to take the answer.
pub const ANSWER_TAKE_WAIT: Duration = Duration::from_secs(5);
/// The largest tool input, as JSON bytes, that the lead may answer.
pub const PERMISSION_INPUT_MAX: usize = 65_536;
/// The longest summary a request carries, in characters.
pub const SUMMARY_MAX: usize = 2_000;
/// The first words of a denial's message.
pub const DENIED_MARKER: &str = "DIALOG DENIED:";

/// The decision for `answer` to the dialog `id` that asked `request`. A
/// denial's message is, word for word:
///
/// ```text
/// DIALOG DENIED: the lead refused this <tool> call (dialog <id>):
/// <request.summary>
/// Reason: <answer.reason>
/// Do not run this command another way, such as in a script, another shell or another tool. Change your approach, or ask your lead.
/// ```
pub fn decision_for(
    id: &Ulid,
    request: &PermissionRequest,
    answer: &PermissionAnswer,
) -> PermissionDecision {
    let message = match answer.verdict {
        DialogVerdict::Approve => None,
        DialogVerdict::Deny => Some(format!(
            "{DENIED_MARKER} the lead refused this {tool} call (dialog {id}):\n\
             {summary}\n\
             Reason: {reason}\n\
             Do not run this command another way, such as in a script, another shell or another tool. \
             Change your approach, or ask your lead.",
            tool = request.tool,
            summary = request.summary,
            reason = answer.reason,
        )),
    };
    PermissionDecision {
        verdict: answer.verdict,
        message,
    }
}

/// `at` as the time of day in UTC, `HH:MM:SSZ`: the "answer by" time of a
/// watch line, short enough to read at a glance.
pub fn clock_time(at: Timestamp) -> String {
    let t = at.as_datetime();
    format!("{:02}:{:02}:{:02}Z", t.hour(), t.minute(), t.second())
}

/// `text` with every character that `sanitize::one_line` drops written as
/// `\u{XXXX}`: every control character except newline and tab, and every
/// character of Unicode's `Default_Ignorable` set, which holds the
/// bidirectional controls and the zero-width characters. So a command cannot
/// move the reader's cursor, hide a character, or show its characters in
/// another order.
pub fn shown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let hidden = (c.is_control() && c != '\n' && c != '\t') || is_invisible(c);
        if hidden {
            out.extend(format!("\\u{{{:x}}}", u32::from(c)).chars());
        } else {
            out.push(c);
        }
    }
    out
}
