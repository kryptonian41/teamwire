//! The tool call a Claude Code session runs now, read from the end of its
//! transcript. Read only, with the same finder and tail reader as the fill.
//!
//! Claude Code writes each `tool_use` block in an assistant line and its
//! `tool_result` (naming `tool_use_id`) in a later user line. A call with no
//! result yet is the one that runs.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;
use teamwire_core::{SessionId, Timestamp, ToolRun};

use crate::fill::{find_transcript, tail_lines};

/// The newest call of `session` that has no result, from `transcript` when
/// given, else from `<claude_dir>/projects/*/<session>.jsonl`.
pub(crate) fn current_tool(
    claude_dir: &Path,
    session: &SessionId,
    transcript: Option<&Path>,
) -> Option<ToolRun> {
    let file = match transcript {
        Some(p) => p.to_path_buf(),
        None => find_transcript(claude_dir, session)?,
    };
    tool_from_tail(&tail_lines(&file)?)
}

/// Walk the lines newest first, keeping the ids that have a result. A
/// prompt a person typed starts a new turn, so no call before it still runs.
fn tool_from_tail(tail: &str) -> Option<ToolRun> {
    let mut done: HashSet<String> = HashSet::new();
    for line in tail.lines().rev() {
        let Ok(doc) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if doc.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let content = doc.get("message").and_then(|m| m.get("content"));
        match doc.get("type").and_then(Value::as_str) {
            Some("user") => {
                let Some(blocks) = content.and_then(Value::as_array) else {
                    if content.is_some_and(Value::is_string) {
                        return None;
                    }
                    continue;
                };
                done.extend(
                    blocks
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                        .filter_map(|b| b.get("tool_use_id").and_then(Value::as_str))
                        .map(str::to_owned),
                );
            }
            Some("assistant") => {
                let blocks = content.and_then(Value::as_array);
                let open = blocks.into_iter().flatten().rev().find(|b| {
                    b.get("type").and_then(Value::as_str) == Some("tool_use")
                        && b.get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| !done.contains(id))
                });
                if let Some(block) = open {
                    // A call whose name or start cannot be read gives none,
                    // never a guess.
                    let name = block.get("name").and_then(Value::as_str)?;
                    let at = doc.get("timestamp").and_then(Value::as_str)?;
                    return Some(ToolRun {
                        name: name.to_owned(),
                        started_at: Timestamp::parse(at).ok()?,
                    });
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const USE: &str = r#"{"type":"assistant","timestamp":"2026-11-13T07:00:09.500Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#;
    const PROMPT: &str = r#"{"type":"user","timestamp":"2026-11-13T07:01:00.000Z","message":{"role":"user","content":"stop that"}}"#;
    const SIDE: &str = r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-11-13T07:02:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t9","name":"Grep","input":{}}]}}"#;

    #[test]
    fn a_prompt_after_an_open_call_means_no_call_runs() {
        assert_eq!(tool_from_tail(&format!("{USE}\n{PROMPT}\n")), None);
    }

    #[test]
    fn a_subagents_call_is_not_the_sessions_own() {
        let run = tool_from_tail(&format!("{USE}\n{SIDE}\n")).map(|r| r.name);
        assert_eq!(run.as_deref(), Some("Bash"));
    }

    #[test]
    fn a_call_without_a_readable_start_gives_none() {
        let bad = USE.replace("2026-11-13T07:00:09.500Z", "yesterday");
        assert_eq!(tool_from_tail(&bad), None);
    }
}
