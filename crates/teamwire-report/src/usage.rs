//! Token usage of a member's session, read from its Claude Code transcript.
//! Claude-specific like `final_message`, and it lives here because a layer-4
//! crate may not depend on the layer-2 Claude adapter.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::{Value, json};

/// A transcript larger than this is not read: the hook has a time budget.
const MAX_TRANSCRIPT_BYTES: u64 = 64 * 1024 * 1024;

const KEYS: [(&str, &str); 4] = [
    ("input_tokens", "input"),
    ("output_tokens", "output"),
    ("cache_read_input_tokens", "cache_read"),
    ("cache_creation_input_tokens", "cache_creation"),
];

/// Cumulative usage of the transcript at `path` as `{model: {input, output,
/// cache_read, cache_creation}}`, or `None` when the file is missing,
/// too large, or holds no usage.
///
/// One API response is split over several lines that repeat `message.id`
/// and its usage, so each id counts once. A line that does not parse is
/// skipped.
// shortcut: subagent transcripts are separate files and are not counted; read
// them too when a member's subagent spend matters.
pub(crate) fn session_usage(path: &Path) -> Option<Value> {
    if std::fs::metadata(path).ok()?.len() > MAX_TRANSCRIPT_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    // The last line of an id wins: a streamed response ends with its final count.
    let mut by_id: HashMap<String, (String, [u64; 4])> = HashMap::new();
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let (Some(id), Some(model), Some(usage)) = (
            record.pointer("/message/id").and_then(Value::as_str),
            record.pointer("/message/model").and_then(Value::as_str),
            record.pointer("/message/usage"),
        ) else {
            continue;
        };
        let counts = KEYS.map(|(key, _)| usage.get(key).and_then(Value::as_u64).unwrap_or(0));
        by_id.insert(id.to_owned(), (model.to_owned(), counts));
    }
    let mut totals: BTreeMap<String, [u64; 4]> = BTreeMap::new();
    for (model, counts) in by_id.into_values() {
        let sum = totals.entry(model).or_default();
        for (s, c) in sum.iter_mut().zip(counts) {
            *s = s.saturating_add(c);
        }
    }
    if totals.is_empty() {
        return None;
    }
    let object = totals
        .into_iter()
        .map(|(model, c)| {
            let fields: serde_json::Map<String, Value> = KEYS
                .iter()
                .zip(c)
                .map(|((_, name), n)| ((*name).to_owned(), json!(n)))
                .collect();
            (model, Value::Object(fields))
        })
        .collect::<serde_json::Map<_, _>>();
    Some(Value::Object(object))
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the test writes a transcript file, which is not teamwire state"
)]
mod tests {
    use super::*;

    fn line(id: &str, model: &str, i: u64, o: u64, r: u64, c: u64) -> String {
        json!({"type": "assistant", "message": {"id": id, "model": model, "usage": {
            "input_tokens": i, "output_tokens": o,
            "cache_read_input_tokens": r, "cache_creation_input_tokens": c}}})
        .to_string()
    }

    #[test]
    fn a_repeated_message_id_counts_once_and_models_stay_apart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let body = [
            line("m1", "opus", 2, 10, 100, 5),
            line("m1", "opus", 2, 10, 100, 5),
            "not json".to_owned(),
            json!({"type": "user"}).to_string(),
            line("m2", "opus", 1, 20, 50, 0),
            line("m3", "haiku", 7, 3, 0, 9),
        ]
        .join("\n");
        std::fs::write(&path, body).unwrap();
        assert_eq!(
            session_usage(&path),
            Some(json!({
                "opus": {"input": 3, "output": 30, "cache_read": 150, "cache_creation": 5},
                "haiku": {"input": 7, "output": 3, "cache_read": 0, "cache_creation": 9},
            }))
        );
    }

    #[test]
    fn a_missing_file_or_one_without_usage_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(session_usage(&dir.path().join("none")), None);
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, "{\"type\":\"user\"}\n").unwrap();
        assert_eq!(session_usage(&path), None);
    }
}
