//! The member's final message: the hook's own field first, the transcript on
//! disk only as a fallback, because the file lags the turn (INV-15, REP-7).

use std::path::Path;

use serde_json::Value;
use teamwire_core::{FinalSource, TurnEnd};

/// The final message and where it came from.
pub(crate) fn pick(t: &TurnEnd) -> (Option<String>, FinalSource) {
    if let Some(text) = t.final_message.as_deref().map(str::trim)
        && !text.is_empty()
    {
        return (Some(text.to_owned()), FinalSource::Hook);
    }
    match t.transcript.as_deref().and_then(last_assistant_text) {
        Some(text) => (Some(text), FinalSource::Transcript),
        None => (None, FinalSource::None),
    }
}

/// The text of the last assistant record in a Claude Code transcript (one
/// JSON object per line). A line that does not parse is skipped.
fn last_assistant_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut last = None;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let blocks = record
            .pointer("/message/content")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let joined: String = blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect();
        if !joined.trim().is_empty() {
            last = Some(joined.trim().to_owned());
        }
    }
    last
}
