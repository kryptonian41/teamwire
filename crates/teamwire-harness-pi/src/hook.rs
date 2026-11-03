//! Hook input from teamwire's pi extension, and the reply that holds a
//! session once.
//!
//! pi has no hook commands, so the extension (`extension.rs`) writes this
//! input itself: `session_id`, `hold_active`, `final_message` and
//! `session_file`. The post-tool hook sends only the session fields.

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
        // The extension skips a call another tool made, and pi runs no
        // subagent of its own, so every input is the member's main session.
        subagent: false,
        hold_already_active: doc
            .get("hold_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        final_message: non_blank(&doc, "final_message").map(str::to_owned),
        transcript: non_blank(&doc, "session_file").map(Into::into),
    })
}

/// The reply the extension turns into one more turn that carries `note`.
pub(crate) fn hold_reply(note: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({ "decision": "block", "reason": note })).unwrap_or_default()
}
