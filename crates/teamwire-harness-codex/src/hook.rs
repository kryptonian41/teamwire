//! Hook input from Codex, and the reply that holds a session once.
//!
//! One parser serves the `Stop`, `PostToolUse` and `SessionStart` hooks, because
//! each of their inputs carries `session_id`, and the callers read nothing
//! else that all three share.

use serde_json::{Value, json};
use teamwire_core::{HarnessError, HarnessErrorCode, SessionId, TurnEnd};

fn bad_input(why: impl Into<String>) -> HarnessError {
    HarnessError::new(HarnessErrorCode::BadInput, why)
}

fn non_blank<'a>(doc: &'a Value, key: &str) -> Option<&'a str> {
    doc.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

pub(crate) fn parse_turn_end(stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
    let doc: Value = serde_json::from_slice(stdin).map_err(|e| bad_input(e.to_string()))?;
    if !doc.is_object() {
        return Err(bad_input("the hook input is not a JSON object"));
    }
    let session = non_blank(&doc, "session_id").ok_or_else(|| bad_input("no session_id"))?;
    let session = SessionId::parse(session).map_err(|e| bad_input(e.to_string()))?;

    Ok(TurnEnd {
        session,
        // Codex sends `SubagentStop` for a subagent, so a `Stop` is never one.
        // `PostToolUse` names a subagent's call with `agent_id`.
        subagent: non_blank(&doc, "agent_id").is_some(),
        hold_already_active: doc
            .get("stop_hook_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        final_message: non_blank(&doc, "last_assistant_message").map(str::to_owned),
        transcript: non_blank(&doc, "transcript_path").map(Into::into),
    })
}

/// The JSON Codex honours on exit 0 to keep a session going: the reason is
/// handed to the model as a new prompt and the turn continues.
pub(crate) fn hold_reply(note: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({ "decision": "block", "reason": note })).unwrap_or_default()
}
