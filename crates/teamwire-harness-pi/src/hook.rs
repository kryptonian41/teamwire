//! Hook input from teamwire's pi extension, and the reply that holds a
//! session once.
//!
//! pi has no hook commands, so the extension (`extension.rs`) writes this
//! input itself: `session_id`, `hold_active`, `final_message` and
//! `session_file`. The post-tool hook sends only the session fields. The
//! lead extension's tool hook (`lead_extension.rs`, ADR 0035) sends
//! `session_id`, `tool_name`, `tool_input` and `cwd`.

use std::path::PathBuf;

use serde_json::{Value, json};
use teamwire_core::{HarnessError, HarnessErrorCode, SessionId, ToolAction, ToolCall, TurnEnd};

fn bad_input(why: impl Into<String>) -> HarnessError {
    HarnessError::new(HarnessErrorCode::BadInput, why)
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

/// The lead's tool call, as the lead extension sends it: pi's `edit` and
/// `write` tools name a `path`, and its `bash` tool a `command`.
pub(crate) fn parse_tool_call(stdin: &[u8]) -> Result<ToolCall, HarnessError> {
    let (doc, session) = object_with_session(stdin)?;
    let input = doc.get("tool_input").unwrap_or(&Value::Null);
    let action = match non_blank(&doc, "tool_name") {
        Some("edit" | "write") => non_blank(input, "path").map_or(ToolAction::Other, |p| {
            ToolAction::Edit(vec![PathBuf::from(p)])
        }),
        Some("bash") => non_blank(input, "command")
            .map_or(ToolAction::Other, |c| ToolAction::Shell(c.to_owned())),
        _ => ToolAction::Other,
    };
    Ok(ToolCall {
        session,
        // As for a turn end: the extension never sends another tool's call.
        subagent: false,
        cwd: non_blank(&doc, "cwd").map(PathBuf::from),
        action,
    })
}

/// The reply the extension turns into one more turn that carries `note`.
pub(crate) fn hold_reply(note: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({ "decision": "block", "reason": note })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(doc: &Value) -> ToolCall {
        parse_tool_call(doc.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn edit_and_write_name_their_path_and_bash_its_command() {
        for tool in ["edit", "write"] {
            let got = call(&json!({
                "session_id": "s1", "tool_name": tool, "tool_input": {"path": "src/a.rs"},
                "cwd": "/w",
            }));
            assert_eq!(
                got.action,
                ToolAction::Edit(vec!["src/a.rs".into()]),
                "{tool}"
            );
            assert_eq!(got.cwd, Some(PathBuf::from("/w")));
        }
        let bash = call(&json!({
            "session_id": "s1", "tool_name": "bash", "tool_input": {"command": "bun test"},
        }));
        assert_eq!(bash.action, ToolAction::Shell("bun test".to_owned()));
    }

    #[test]
    fn other_tools_give_nothing_and_a_call_needs_a_session() {
        let read = call(&json!({
            "session_id": "s1", "tool_name": "read", "tool_input": {"path": "src/a.rs"},
        }));
        assert_eq!(read.action, ToolAction::Other);
        assert!(!read.subagent);
        assert!(parse_tool_call(br#"{"tool_name":"bash"}"#).is_err());
    }
}
