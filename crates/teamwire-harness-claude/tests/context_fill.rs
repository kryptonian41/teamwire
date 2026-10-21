//! The fill of a Claude session, read from the end of its transcript.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use teamwire_core::{Fill, Harness, Platform, SessionId};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn claude(dir: &Path) -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: dir.to_path_buf(),
            claude_json: dir.join("claude.json"),
            claude_bin: "claude".to_owned(),
        },
    )
}

fn assistant(model: &str, input: u64, read: u64, create: u64, side: bool) -> String {
    json!({"type":"assistant","isSidechain":side,"message":{"model":model,"usage":{
        "input_tokens":input,"cache_read_input_tokens":read,
        "cache_creation_input_tokens":create,"output_tokens":9999}}})
    .to_string()
}

fn boundary() -> String {
    json!({"type":"system","subtype":"compact_boundary"}).to_string()
}

/// A transcript holding `lines`, as `<claude dir>/projects/-x/s1.jsonl`.
fn transcript(lines: &[String]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("projects/-x/s1.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    FakePlatform::new()
        .write_atomic(&path, (lines.join("\n") + "\n").as_bytes())
        .unwrap();
    (dir, path)
}

fn fill(dir: &Path, path: Option<&Path>) -> Option<Fill> {
    let sid = SessionId::parse("s1").unwrap();
    claude(dir).context_fill(&sid, path).unwrap()
}

#[test]
fn sums_input_and_cache_tokens_of_the_newest_line() {
    let (d, p) = transcript(&[
        assistant("claude-opus-5-5", 1, 1, 1, false),
        assistant("claude-opus-5-5", 3, 700_000, 20_088, false),
    ]);
    let want = Some(Fill {
        tokens: 720_091,
        window: 1_000_000,
    });
    assert_eq!(fill(d.path(), Some(&p)), want);
    // Without a path, the session's own transcript is found.
    assert_eq!(fill(d.path(), None), want);
}

#[test]
fn skips_a_side_chain_line_newer_than_the_main_chain() {
    let (d, p) = transcript(&[
        assistant("claude-opus-5-5", 10, 0, 0, false),
        assistant("claude-opus-5-5", 999, 0, 0, true),
    ]);
    assert_eq!(fill(d.path(), Some(&p)).unwrap().tokens, 10);
}

#[test]
fn a_compaction_boundary_after_the_last_assistant_line_gives_none() {
    let (d, p) = transcript(&[assistant("claude-opus-5-5", 10, 0, 0, false), boundary()]);
    assert_eq!(fill(d.path(), Some(&p)), None);
}

#[test]
fn an_assistant_line_after_the_boundary_counts() {
    let (d, p) = transcript(&[boundary(), assistant("claude-opus-5-5", 10, 0, 0, false)]);
    assert_eq!(fill(d.path(), Some(&p)).unwrap().tokens, 10);
}

#[test]
fn an_empty_or_missing_file_gives_none() {
    let (d, p) = transcript(&[]);
    assert_eq!(fill(d.path(), Some(&p)), None);
    assert_eq!(
        fill(d.path(), Some(&p.with_file_name("missing.jsonl"))),
        None
    );
    assert_eq!(fill(&d.path().join("nowhere"), None), None);
}

#[test]
fn an_unknown_model_counts_as_a_small_window() {
    let (d, p) = transcript(&[assistant("mystery-1", 10, 0, 0, false)]);
    assert_eq!(fill(d.path(), Some(&p)).unwrap().window, 200_000);
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a sparse file cannot be made through Platform::write_atomic"
)]
fn a_huge_file_is_read_only_from_its_tail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    // An assistant line at the start, a sparse 200 MB hole, then a line that
    // is no reading. Only a read of the whole file could find the first line.
    writeln!(f, "{}", assistant("claude-opus-5-5", 7, 0, 0, false)).unwrap();
    f.seek(SeekFrom::Start(200 << 20)).unwrap();
    writeln!(f, "\n{}", json!({"type":"user"})).unwrap();
    drop(f);
    assert_eq!(fill(dir.path(), Some(&path)), None);
}
