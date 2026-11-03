//! The folder a session runs in, the plain resume of the lead, and the fill of
//! a session's context: read from the live session file or the transcript;
//! nothing is written.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use teamwire_core::{Fill, Harness, Platform, SessionId};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn claude(p: &Arc<FakePlatform>, dir: &Path) -> ClaudeCode {
    ClaudeCode::new(
        p.clone(),
        ClaudeConfig {
            claude_dir: dir.to_path_buf(),
            claude_json: dir.join("claude.json"),
            claude_bin: "claude".to_owned(),
        },
    )
}

fn put(p: &FakePlatform, path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    p.write_atomic(path, text.as_bytes()).unwrap();
}

#[test]
fn the_live_session_file_names_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let p = Arc::new(FakePlatform::new());
    let file = json!({ "pid": 7, "sessionId": "s-1", "cwd": "/work/lead" });
    put(&p, &dir.path().join("sessions/7.json"), &file.to_string());
    let other = json!({ "pid": 8, "sessionId": "s-2", "cwd": "/elsewhere" });
    put(&p, &dir.path().join("sessions/8.json"), &other.to_string());
    assert_eq!(
        claude(&p, dir.path()).session_cwd(&sid("s-1")).unwrap(),
        Some(PathBuf::from("/work/lead"))
    );
}

#[test]
fn the_transcript_names_the_folder_when_no_live_file_does() {
    let dir = tempfile::tempdir().unwrap();
    let p = Arc::new(FakePlatform::new());
    let lines = [
        json!({ "type": "summary" }).to_string(),
        json!({ "type": "user", "cwd": "/work/old" }).to_string(),
    ]
    .join("\n");
    put(&p, &dir.path().join("projects/-work-old/s-9.jsonl"), &lines);
    let claude = claude(&p, dir.path());
    assert_eq!(
        claude.session_cwd(&sid("s-9")).unwrap(),
        Some(PathBuf::from("/work/old"))
    );
    assert_eq!(claude.session_cwd(&sid("unknown")).unwrap(), None);
}

#[test]
fn the_lead_resumes_with_a_plain_resume() {
    let dir = tempfile::tempdir().unwrap();
    let p = Arc::new(FakePlatform::new());
    assert_eq!(
        claude(&p, dir.path())
            .lead_resume_argv(&sid("s-1"))
            .unwrap(),
        ["claude", "--resume", "s-1"]
    );
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
    put(&FakePlatform::new(), &path, &(lines.join("\n") + "\n"));
    (dir, path)
}

fn fill(dir: &Path, path: Option<&Path>) -> Option<Fill> {
    claude(&Arc::new(FakePlatform::new()), dir)
        .context_fill(&sid("s1"), path)
        .unwrap()
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
fn a_huge_file_is_read_only_from_its_tail() {
    // An assistant line at the start, 8 MiB of filler (more than the tail
    // that is read), then a line that is no reading. Only a read of the
    // whole file could find the first line.
    let filler = "{\"type\":\"user\"}\n".repeat(8 * 1024 * 1024 / 16);
    let (d, p) = transcript(&[
        assistant("claude-opus-5-5", 7, 0, 0, false),
        filler,
        json!({"type":"user"}).to_string(),
    ]);
    assert_eq!(fill(d.path(), Some(&p)), None);
}

#[test]
fn api_error_lines_are_no_reading_and_the_real_one_before_them_counts() {
    let error = json!({"type":"assistant","isSidechain":false,"isApiErrorMessage":true,
        "message":{"model":"<synthetic>","usage":{"input_tokens":0,
        "cache_read_input_tokens":0,"cache_creation_input_tokens":0}}})
    .to_string();
    let (d, p) = transcript(&[
        assistant("claude-fable-5-1", 1, 900_000, 17_370, false),
        error.clone(),
        error,
    ]);
    assert_eq!(
        fill(d.path(), Some(&p)),
        Some(Fill {
            tokens: 917_371,
            window: 1_000_000
        })
    );
}

#[test]
fn an_assistant_line_without_a_message_is_skipped() {
    let (d, p) = transcript(&[
        assistant("claude-opus-5-5", 10, 0, 0, false),
        json!({"type":"assistant","isSidechain":false}).to_string(),
    ]);
    assert_eq!(fill(d.path(), Some(&p)).unwrap().tokens, 10);
}
