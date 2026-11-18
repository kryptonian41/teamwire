//! Claude Code's `PermissionRequest` hook (ADR 0052): the input it sends
//! when a permission dialog opens, and the one reply teamwire prints, which
//! allows this one call or denies it with the lead's text.
//!
//! Never: a reply with `updatedPermissions` (a lasting grant),
//! `updatedInput` (a changed call) or `interrupt` (a stopped turn); an
//! answerable request for a tool other than Bash, Write, Edit and `WebFetch`,
//! or for an input teamwire cannot show whole.

use serde_json::Value;
use teamwire_core::{
    DialogVerdict, HarnessError, HarnessErrorCode, PERMISSION_INPUT_MAX, PermissionDecision,
    PermissionRequest, SUMMARY_MAX, SessionId,
};

fn bad_input(why: impl Into<String>) -> HarnessError {
    HarnessError::new(HarnessErrorCode::BadInput, why)
}

fn non_blank<'a>(doc: &'a Value, key: &str) -> Option<&'a str> {
    doc.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

/// The input key each answerable tool must carry, and its summary.
fn summary_of(tool: &str, input: &Value) -> Result<String, String> {
    let key = match tool {
        "Bash" => "command",
        "Write" | "Edit" => "file_path",
        "WebFetch" => "url",
        _ => return Err("teamwire answers only Bash, Write, Edit and WebFetch calls".to_owned()),
    };
    let Some(value) = non_blank(input, key) else {
        return Err(format!("the {tool} input has no {key}"));
    };
    Ok(match tool {
        "Write" => {
            let n = input
                .get("content")
                .and_then(Value::as_str)
                .map_or(0, str::len);
            format!("Write {value} ({n} bytes)")
        }
        "Edit" | "WebFetch" => format!("{tool} {value}"),
        _ => value.to_owned(),
    })
}

/// `text` cut to `SUMMARY_MAX` characters, on a character boundary.
fn cut(text: &str) -> String {
    text.chars().take(SUMMARY_MAX).collect()
}

pub(crate) fn parse(stdin: &[u8]) -> Result<PermissionRequest, HarnessError> {
    let doc: Value = serde_json::from_slice(stdin).map_err(|e| bad_input(e.to_string()))?;
    if doc.get("hook_event_name").and_then(Value::as_str) != Some("PermissionRequest") {
        return Err(bad_input("the hook input is not a PermissionRequest"));
    }
    let session = non_blank(&doc, "session_id").ok_or_else(|| bad_input("no session_id"))?;
    let session = SessionId::parse(session).map_err(|e| bad_input(e.to_string()))?;
    let tool = doc
        .get("tool_name")
        .and_then(Value::as_str)
        .ok_or_else(|| bad_input("no tool_name"))?
        .to_owned();
    let input = doc
        .get("tool_input")
        .filter(|v| v.is_object())
        .ok_or_else(|| bad_input("tool_input is not an object"))?;
    // `agent_id` marks a call from a subagent, as in the lead's tool hook.
    let subagent = non_blank(&doc, "agent_id").is_some();
    let size = serde_json::to_vec(input).map_or(usize::MAX, |b| b.len());
    let (summary, why_not) = match summary_of(&tool, input) {
        Ok(summary) => (summary, None),
        Err(why) => (tool.clone(), Some(why)),
    };
    let (input, why_not) = if size > PERMISSION_INPUT_MAX {
        (
            None,
            Some(format!(
                "the input is {size} bytes, over the 64 KiB teamwire shows whole"
            )),
        )
    } else {
        (Some(input.clone()), why_not)
    };
    Ok(PermissionRequest {
        session,
        subagent,
        tool,
        summary: cut(&summary),
        input,
        why_not,
    })
}

/// The reply's text, written out so its keys keep Claude Code's documented
/// order; `serde_json` would sort them.
pub(crate) fn reply(decision: &PermissionDecision) -> Vec<u8> {
    let choice = match decision.verdict {
        DialogVerdict::Approve => r#"{"behavior":"allow"}"#.to_owned(),
        DialogVerdict::Deny => format!(
            r#"{{"behavior":"deny","message":{}}}"#,
            Value::String(decision.message.clone().unwrap_or_default())
        ),
    };
    format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{choice}}}}}"#
    )
    .into_bytes()
}
