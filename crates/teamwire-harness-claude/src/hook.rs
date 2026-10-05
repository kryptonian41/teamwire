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

/// True when the Stop input lists a background task that may still run.
///
/// The shape of `background_tasks` was not confirmed against a live Stop
/// input. A task with no readable `status` counts as running, so an
/// unknown shape still reads as a pause. Capture a real input with
/// `examples/live_check.rs` to settle the field names and status values.
fn has_running_task(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Array(tasks)) => tasks.iter().any(|task| {
            let status = task.get("status").and_then(Value::as_str);
            !matches!(
                status,
                Some(
                    "done"
                        | "completed"
                        | "finished"
                        | "failed"
                        | "stopped"
                        | "exited"
                        | "cancelled"
                        | "killed"
                        | "ended"
                )
            )
        }),
        other => present(other),
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

    // `agent_id` marks a subagent's stop. A `background_tasks` list with a
    // running task means the session only paused while that task runs: its
    // work is not finished. A list of finished tasks is a real stop. `TurnEnd` has no field for "paused", so both read as
    // `subagent`, which callers treat as "report nothing".
    let subagent = present(doc.get("agent_id")) || has_running_task(doc.get("background_tasks"));

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
/// the form the Python Stop hook has run in production, and the one ADR 0007
/// names.
pub(crate) fn hold_reply(note: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "hookSpecificOutput": { "hookEventName": "Stop", "additionalContext": note }
    }))
    .unwrap_or_default()
}
