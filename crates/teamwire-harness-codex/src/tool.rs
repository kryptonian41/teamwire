//! The tool call a Codex session runs now, read from the end of its rollout.
//! Read only, with the same finder and tail reader as the fill.
//!
//! A rollout line is `{"timestamp":..,"type":"response_item","payload":{..}}`,
//! and the payload is a `ResponseItem` (openai/codex
//! `codex-rs/protocol/src/models.rs`, tagged by `type` in snake case). A call
//! is a `function_call`, `custom_tool_call` or `local_shell_call` with a
//! `call_id`; its result is a `function_call_output` or
//! `custom_tool_call_output` naming that `call_id`. The shape comes from that
//! published source; no live Codex rollout was read.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;
use teamwire_core::{SessionId, Timestamp, ToolRun};

use crate::session::{rollout_path, tail_text};

/// The newest call of `session` that has no output yet.
pub(crate) fn current_tool(codex_home: &Path, session: &SessionId) -> Option<ToolRun> {
    tool_from_tail(&tail_text(&rollout_path(codex_home, session)?)?)
}

/// Walk the lines newest first, keeping the call ids that have an output. A
/// user message starts a new turn, so no call before it still runs. An entry
/// that cannot be read gives none, never a guess.
fn tool_from_tail(tail: &str) -> Option<ToolRun> {
    let mut done: HashSet<String> = HashSet::new();
    for line in tail.lines().rev() {
        let Ok(doc) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if doc.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let Some(item) = doc.get("payload") else {
            continue;
        };
        let call_id = || item.get("call_id").and_then(Value::as_str);
        let name = match item.get("type").and_then(Value::as_str) {
            Some("function_call_output" | "custom_tool_call_output") => {
                done.insert(call_id()?.to_owned());
                continue;
            }
            Some("message") if item.get("role").and_then(Value::as_str) == Some("user") => {
                return None;
            }
            Some("function_call" | "custom_tool_call") => item.get("name")?.as_str()?,
            Some("local_shell_call") => "local_shell",
            _ => continue,
        };
        if done.contains(call_id()?) {
            continue;
        }
        let at = doc.get("timestamp")?.as_str()?;
        return Some(ToolRun {
            name: name.to_owned(),
            started_at: Timestamp::parse(at).ok()?,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALL: &str = r#"{"timestamp":"2026-10-18T10:00:30.000Z","type":"response_item","payload":{"type":"local_shell_call","call_id":"c1","status":"in_progress","action":{"type":"exec","command":["ls"]}}}"#;
    const USER: &str = r#"{"timestamp":"2026-10-18T10:01:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"stop"}]}}"#;

    #[test]
    fn a_local_shell_call_is_named_local_shell() {
        let run = tool_from_tail(CALL).map(|r| r.name);
        assert_eq!(run.as_deref(), Some("local_shell"));
    }

    #[test]
    fn a_user_message_after_an_open_call_means_no_call_runs() {
        assert_eq!(tool_from_tail(&format!("{CALL}\n{USER}\n")), None);
    }

    #[test]
    fn a_call_without_a_call_id_gives_none() {
        assert_eq!(
            tool_from_tail(&CALL.replace(r#""call_id":"c1","#, "")),
            None
        );
    }
}
