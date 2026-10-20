//! Token usage of a member's session, read from its Claude Code transcript.
//! Claude-specific like `final_message`, and it lives here because a layer-4
//! crate may not depend on the layer-2 Claude adapter.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::{Value, json};

/// A transcript larger than this is not read: the hook has a time budget.
pub(crate) const MAX_TRANSCRIPT_BYTES: u64 = 64 * 1024 * 1024;

/// The bytes of the transcript file at `path`, or `None` when it is not a
/// regular file, is larger than `limit` bytes or cannot be read. Only a
/// regular file is opened: opening a FIFO would block the Stop hook.
pub(crate) fn read_transcript(path: &Path, limit: u64) -> Option<Vec<u8>> {
    std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file() && m.len() <= limit)?;
    std::fs::read(path).ok()
}

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
///
/// Subagent transcripts sit in `<session-id>/subagents/*.jsonl` beside the
/// main file and use the same line shape. They count too, while the sum of
/// the sizes stays under the size cap. A subagent file that would pass the cap,
/// is not a regular file or cannot be read is skipped; the rest still count.
pub(crate) fn session_usage(path: &Path) -> Option<Value> {
    let mut files = vec![path.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(path.with_extension("").join("subagents")) {
        let mut subagents: Vec<_> = entries
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        subagents.sort();
        files.extend(subagents);
    }
    let mut size = 0_u64;
    let mut texts = Vec::new();
    for (i, file) in files.iter().enumerate() {
        match read_transcript(file, MAX_TRANSCRIPT_BYTES.saturating_sub(size)) {
            Some(bytes) => {
                size = size.saturating_add(bytes.len() as u64);
                texts.push(String::from_utf8_lossy(&bytes).into_owned());
            }
            // The main transcript decides whether the session has usage; a
            // subagent file that is too big or unreadable is only left out.
            None if i == 0 => return None,
            None => {}
        }
    }
    // The last line of an id wins: a streamed response ends with its final count.
    let mut by_id: HashMap<String, (String, [u64; 4])> = HashMap::new();
    for line in texts.iter().flat_map(|t| t.lines()) {
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
    fn subagent_transcripts_add_to_the_sum_and_a_repeated_id_counts_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, line("m1", "opus", 1, 10, 0, 0)).unwrap();
        let sub = dir.path().join("t").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        let body = [
            line("m1", "opus", 1, 10, 0, 0),
            line("s1", "opus", 2, 5, 0, 0),
        ]
        .join("\n");
        std::fs::write(sub.join("agent-x.jsonl"), body).unwrap();
        std::fs::write(sub.join("agent-x.meta.json"), "{}").unwrap();
        assert_eq!(
            session_usage(&path),
            Some(json!({"opus": {"input": 3, "output": 15, "cache_read": 0, "cache_creation": 0}}))
        );
    }

    #[test]
    fn a_subagent_file_that_is_unreadable_or_not_a_file_is_skipped_and_the_rest_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, line("m1", "opus", 1, 10, 0, 0)).unwrap();
        let sub = dir.path().join("t").join("subagents");
        std::fs::create_dir_all(sub.join("agent-dir.jsonl")).unwrap();
        std::fs::write(sub.join("agent-ok.jsonl"), line("s1", "opus", 2, 5, 0, 0)).unwrap();
        assert_eq!(
            session_usage(&path),
            Some(json!({"opus": {"input": 3, "output": 15, "cache_read": 0, "cache_creation": 0}}))
        );
    }

    #[test]
    fn a_subagent_file_that_would_pass_the_cap_is_skipped_and_the_main_file_counts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, line("m1", "opus", 1, 10, 0, 0)).unwrap();
        let sub = dir.path().join("t").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("agent-ok.jsonl"), line("s1", "opus", 2, 5, 0, 0)).unwrap();
        // A sparse file: it takes no disk, and its length alone passes the cap.
        let big = std::fs::File::create(sub.join("agent-big.jsonl")).unwrap();
        big.set_len(MAX_TRANSCRIPT_BYTES).unwrap();
        assert_eq!(
            session_usage(&path),
            Some(json!({"opus": {"input": 3, "output": 15, "cache_read": 0, "cache_creation": 0}}))
        );
    }

    #[test]
    fn a_missing_file_or_one_without_usage_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        // A path that is not a regular file is never opened.
        assert_eq!(session_usage(dir.path()), None);
        assert_eq!(session_usage(&dir.path().join("none")), None);
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, "{\"type\":\"user\"}\n").unwrap();
        assert_eq!(session_usage(&path), None);
    }
}
