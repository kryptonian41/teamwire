//! The Stop hook: what Claude Code sends when a turn ends, and the reply
//! that holds the session once. The lead's tool hook reads the same input
//! with the tool's name and input added (ADR 0035).

use std::path::PathBuf;

use serde_json::{Value, json};
use teamwire_core::{HarnessError, HarnessErrorCode, SessionId, ToolAction, ToolCall, TurnEnd};

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

/// The hook input as a JSON object, and the session it names.
fn object_with_session(stdin: &[u8]) -> Result<(Value, SessionId), HarnessError> {
    let doc: Value = serde_json::from_slice(stdin).map_err(|e| bad_input(e.to_string()))?;
    if !doc.is_object() {
        return Err(bad_input("the hook input is not a JSON object"));
    }
    let session = non_blank(&doc, "session_id").ok_or_else(|| bad_input("no session_id"))?;
    let session = SessionId::parse(session).map_err(|e| bad_input(e.to_string()))?;
    Ok((doc, session))
}

pub(crate) fn parse_turn_end(stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
    let (doc, session) = object_with_session(stdin)?;

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

/// A `PostToolUse` input for the lead's tool hook: the file an edit tool
/// writes, or the command the Bash tool runs.
pub(crate) fn parse_tool_call(stdin: &[u8]) -> Result<ToolCall, HarnessError> {
    let (doc, session) = object_with_session(stdin)?;
    let input = doc.get("tool_input").unwrap_or(&Value::Null);
    let action = match non_blank(&doc, "tool_name") {
        Some("Edit" | "Write" | "MultiEdit") => edit(non_blank(input, "file_path")),
        Some("NotebookEdit") => edit(non_blank(input, "notebook_path")),
        Some("Bash") => non_blank(input, "command")
            .map_or(ToolAction::Other, |c| ToolAction::Shell(c.to_owned())),
        _ => ToolAction::Other,
    };
    Ok(ToolCall {
        session,
        subagent: present(doc.get("agent_id")),
        cwd: non_blank(&doc, "cwd").map(PathBuf::from),
        action,
    })
}

fn edit(path: Option<&str>) -> ToolAction {
    path.map_or(ToolAction::Other, |p| {
        ToolAction::Edit(vec![PathBuf::from(p)])
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

#[cfg(test)]
mod tests {
    use super::*;

    fn call(doc: &Value) -> ToolCall {
        parse_tool_call(doc.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn each_edit_tool_names_the_file_it_writes() {
        for (tool, key) in [
            ("Edit", "file_path"),
            ("Write", "file_path"),
            ("MultiEdit", "file_path"),
            ("NotebookEdit", "notebook_path"),
        ] {
            let got = call(&json!({
                "session_id": "s1", "tool_name": tool, "tool_input": {key: "/r/a.rs"},
                "cwd": "/r",
            }));
            assert_eq!(
                got.action,
                ToolAction::Edit(vec!["/r/a.rs".into()]),
                "{tool}"
            );
            assert_eq!(got.cwd, Some(PathBuf::from("/r")));
            assert!(!got.subagent);
        }
    }

    #[test]
    fn bash_gives_its_command_and_other_tools_give_nothing() {
        let bash = call(&json!({
            "session_id": "s1", "tool_name": "Bash", "tool_input": {"command": "cargo test"},
        }));
        assert_eq!(bash.action, ToolAction::Shell("cargo test".to_owned()));
        let read = call(&json!({
            "session_id": "s1", "tool_name": "Read", "tool_input": {"file_path": "/r/a.rs"},
        }));
        assert_eq!(read.action, ToolAction::Other);
        // An edit with no path says nothing it could be judged by.
        let blank = call(&json!({"session_id": "s1", "tool_name": "Write", "tool_input": {}}));
        assert_eq!(blank.action, ToolAction::Other);
    }

    #[test]
    fn a_subagents_call_is_marked_and_a_call_needs_a_session() {
        let sub = call(&json!({
            "session_id": "s1", "agent_id": "a1", "tool_name": "Bash",
            "tool_input": {"command": "ls"},
        }));
        assert!(sub.subagent);
        for junk in [&b"not json"[..], b"[1]", br#"{"tool_name":"Bash"}"#] {
            assert!(parse_tool_call(junk).is_err());
        }
    }
}
