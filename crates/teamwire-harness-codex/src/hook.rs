//! Hook input from Codex, and the reply that holds a session once.
//!
//! One parser serves the `Stop`, `PostToolUse` and `SessionStart` hooks, because
//! each of their inputs carries `session_id`, and the callers read nothing
//! else that all three share. The lead's tool hook also reads a
//! `PostToolUse` input's tool name and input (ADR 0035).

use std::path::PathBuf;

use serde_json::{Value, json};
use teamwire_core::{HarnessError, HarnessErrorCode, SessionId, ToolAction, ToolCall, TurnEnd};

/// The lines of an `apply_patch` patch that name a file the patch writes.
const PATCH_HEADERS: [&str; 4] = [
    "*** Add File: ",
    "*** Update File: ",
    "*** Delete File: ",
    "*** Move to: ",
];

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

/// A `PostToolUse` input for the lead's tool hook. Codex names its shell
/// tool `Bash`, as Claude Code does. A patch can come from Codex's patch
/// tool or through the shell, so the files a patch names win over the
/// command that carried it.
// shortcut: the patch headers and the `Bash` name come from Codex's hook
// input as teamwire's tests record it, not from a live Codex lead; add a
// tool here when a live run shows one.
pub(crate) fn parse_tool_call(stdin: &[u8]) -> Result<ToolCall, HarnessError> {
    let (doc, session) = object_with_session(stdin)?;
    let input = doc.get("tool_input").unwrap_or(&Value::Null);
    let mut texts = Vec::new();
    strings_in(input, &mut texts);
    let patched: Vec<PathBuf> = texts
        .iter()
        .flat_map(|text| text.lines())
        .filter_map(patched_file)
        .collect();
    let action = if !patched.is_empty() {
        ToolAction::Edit(patched)
    } else if non_blank(&doc, "tool_name") == Some("Bash") {
        command(input).map_or(ToolAction::Other, ToolAction::Shell)
    } else {
        ToolAction::Other
    };
    Ok(ToolCall {
        session,
        subagent: non_blank(&doc, "agent_id").is_some(),
        cwd: non_blank(&doc, "cwd").map(PathBuf::from),
        action,
    })
}

/// Every string anywhere in `value`.
fn strings_in<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => {
            for item in items {
                strings_in(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                strings_in(item, out);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn patched_file(line: &str) -> Option<PathBuf> {
    PATCH_HEADERS
        .iter()
        .find_map(|header| line.trim_start().strip_prefix(header))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// The shell command: one string, or a list of words joined with spaces.
fn command(input: &Value) -> Option<String> {
    match input.get("command")? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Array(words) => {
            let words: Vec<&str> = words.iter().filter_map(Value::as_str).collect();
            (!words.is_empty()).then(|| words.join(" "))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
            None
        }
    }
}

/// The JSON Codex honours on exit 0 to keep a session going: the reason is
/// handed to the model as a new prompt and the turn continues.
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
    fn bash_gives_its_command_as_one_string_or_as_words() {
        let one = call(&json!({
            "session_id": "s1", "tool_name": "Bash", "tool_input": {"command": "cargo test"},
            "cwd": "/w",
        }));
        assert_eq!(one.action, ToolAction::Shell("cargo test".to_owned()));
        assert_eq!(one.cwd, Some(PathBuf::from("/w")));
        let words = call(&json!({
            "session_id": "s1", "tool_name": "Bash",
            "tool_input": {"command": ["bash", "-lc", "git commit -m x"]},
        }));
        assert_eq!(
            words.action,
            ToolAction::Shell("bash -lc git commit -m x".to_owned())
        );
    }

    #[test]
    fn a_patch_names_the_files_it_writes_whichever_tool_carried_it() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n\
                     *** Add File: src/b.rs\n+z\n*** End Patch";
        let tool = call(&json!({
            "session_id": "s1", "tool_name": "apply_patch", "tool_input": {"input": patch},
        }));
        assert_eq!(
            tool.action,
            ToolAction::Edit(vec!["src/a.rs".into(), "src/b.rs".into()])
        );
        let shell = call(&json!({
            "session_id": "s1", "tool_name": "Bash",
            "tool_input": {"command": format!("apply_patch <<'EOF'\n{patch}\nEOF")},
        }));
        assert_eq!(shell.action, tool.action);
    }

    #[test]
    fn other_tools_give_nothing_and_a_subagent_is_marked() {
        let read = call(&json!({
            "session_id": "s1", "tool_name": "Read", "tool_input": {"path": "src/a.rs"},
        }));
        assert_eq!(read.action, ToolAction::Other);
        let sub = call(&json!({
            "session_id": "s1", "agent_id": "a1", "tool_name": "Bash",
            "tool_input": {"command": "ls"},
        }));
        assert!(sub.subagent);
        assert!(parse_tool_call(br#"{"tool_name":"Bash"}"#).is_err());
    }
}
