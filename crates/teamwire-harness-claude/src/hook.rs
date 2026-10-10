//! The Stop hook: what Claude Code sends when a turn ends, and the reply
//! that holds the session once.

use serde_json::{Value, json};
use teamwire_core::{HarnessError, HarnessErrorCode, SessionId, TurnEnd};

fn bad_input(why: impl Into<String>) -> HarnessError {
    HarnessError::new(HarnessErrorCode::BadInput, why)
}

/// True for a JSON value that says something: not null, not an empty
/// string, list or object.
fn present(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(_) => true,
    }
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

    // `agent_id` marks a subagent's stop. `background_tasks` is ignored: a
    // task the member left running says nothing about whether its work is
    // done, and a stuck task stays "running" for every later stop. The
    // member's own `done` record is the signal that it finished.
    let subagent = present(doc.get("agent_id"));

    Ok(TurnEnd {
        session,
        subagent,
        hold_already_active: doc
            .get("stop_hook_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        final_message: non_blank(&doc, "last_assistant_message").map(str::to_owned),
        transcript: non_blank(&doc, "transcript_path").map(Into::into),
    })
}

/// The JSON Claude Code honours on exit 0 to keep a session going: the note
/// is handed to the model as added context and the turn continues. This is
/// the form ADR 0007 names.
pub(crate) fn hold_reply(note: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "hookSpecificOutput": { "hookEventName": "Stop", "additionalContext": note }
    }))
    .unwrap_or_default()
}
