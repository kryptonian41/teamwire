//! How full a Claude Code session's context window is, read from the end of
//! its transcript. Read only, and only a bounded tail of the file.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use teamwire_core::{Fill, SessionId};

/// The most of a transcript's end that is read. A transcript grows without
/// bound, and the newest assistant line is near its end.
const TAIL_BYTES: u64 = 4 << 20;

/// The window when the model is unknown or 1M context is off. A small guess
/// makes the handoff note fire early. A large guess could stop it firing.
pub(crate) const SMALL_WINDOW: u64 = 200_000;
pub(crate) const LARGE_WINDOW: u64 = 1_000_000;

/// The window of `model`, or `None` when the model is not known.
///
/// Source: <https://code.claude.com/docs/en/model-config> ("Extended context").
/// Opus 4.7 and later, Sonnet 5 and later, Haiku 5.5 and Fable run with 1M by
/// default. Opus 4.6 and Sonnet 4.6 need the `[1m]` suffix, which a transcript
/// does not record, so only a model string that carries the suffix counts them
/// large. `CLAUDE_CODE_DISABLE_1M_CONTEXT=1` makes every model 200K.
pub(crate) fn model_window(model: &str, disable_1m: bool) -> Option<u64> {
    if disable_1m {
        return Some(SMALL_WINDOW);
    }
    let m = model.to_ascii_lowercase();
    if m.ends_with("[1m]") {
        return Some(LARGE_WINDOW);
    }
    let family = ["opus", "sonnet", "haiku", "fable"]
        .into_iter()
        .find(|f| m.contains(f))?;
    let (major, minor) = version_after(&m, family);
    // An old-style id (`claude-3-5-sonnet-20241022`) puts its version before
    // the family, so only a date follows it. Those models are all 200K.
    if major >= 100 {
        return Some(SMALL_WINDOW);
    }
    let large = match family {
        "fable" => true,
        "opus" => (major, minor) >= (4, 7),
        "sonnet" => major >= 5,
        _ => (major, minor) >= (5, 5),
    };
    // A bare alias (`opus`) names the current model, which has 1M.
    Some(if large || major == 0 && minor == 0 {
        LARGE_WINDOW
    } else {
        SMALL_WINDOW
    })
}

/// The version numbers that follow the family name: `opus-4-7` is (4, 7),
/// `sonnet-5` is (5, 0). A date such as `-20260101` is not a minor version.
fn version_after(model: &str, family: &str) -> (u64, u64) {
    let rest = model.split_once(family).map_or("", |(_, r)| r);
    let mut nums = rest
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().unwrap_or(0));
    let major = nums.next().unwrap_or(0);
    let minor = nums.next().filter(|n| *n < 100).unwrap_or(0);
    (major, minor)
}

pub(crate) fn disable_1m_from_env() -> bool {
    std::env::var("CLAUDE_CODE_DISABLE_1M_CONTEXT").is_ok_and(|v| v == "1")
}

/// The fill of `session`: from `transcript` when given, else from
/// `<claude_dir>/projects/*/<session>.jsonl`.
pub(crate) fn context_fill(
    claude_dir: &Path,
    session: &SessionId,
    transcript: Option<&Path>,
) -> Option<Fill> {
    let file = match transcript {
        Some(p) => p.to_path_buf(),
        None => find_transcript(claude_dir, session)?,
    };
    fill_from_tail(&tail_lines(&file)?, disable_1m_from_env())
}

pub(crate) fn find_transcript(claude_dir: &Path, session: &SessionId) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(claude_dir.join("projects"))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|d| d.join(format!("{}.jsonl", session.as_str())))
        .find(|f| f.is_file())
}

/// The last `TAIL_BYTES` of the file as text, without its first line when the
/// cut falls inside it.
pub(crate) fn tail_lines(file: &Path) -> Option<String> {
    let mut f = std::fs::File::open(file).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.take(TAIL_BYTES).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start == 0 {
        return Some(text);
    }
    text.split_once('\n').map(|(_, rest)| rest.to_owned())
}

/// Walk the lines newest first. A compaction boundary seen before any
/// assistant line means the numbers before it are gone.
fn fill_from_tail(tail: &str, disable_1m: bool) -> Option<Fill> {
    for line in tail.lines().rev() {
        let Ok(doc) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if doc.get("subtype").and_then(Value::as_str) == Some("compact_boundary") {
            return None;
        }
        if doc.get("type").and_then(Value::as_str) != Some("assistant")
            || doc.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let Some(message) = doc.get("message") else {
            continue;
        };
        // Claude Code writes an API error ("Prompt is too long") as a main-chain
        // assistant line with model `<synthetic>` and every usage field 0. It
        // is no reading, and counting it would show a full member as empty.
        if doc.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true)
            || message.get("model").and_then(Value::as_str) == Some("<synthetic>")
        {
            continue;
        }
        let Some(usage) = message.get("usage") else {
            continue;
        };
        let count = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
        let tokens = count("input_tokens")
            + count("cache_read_input_tokens")
            + count("cache_creation_input_tokens");
        let window = message
            .get("model")
            .and_then(Value::as_str)
            .and_then(|m| model_window(m, disable_1m))
            .unwrap_or(SMALL_WINDOW);
        return Some(Fill { tokens, window });
    }
    None
}

#[cfg(test)]
mod tests;
