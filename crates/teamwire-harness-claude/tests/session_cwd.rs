//! The folder a session runs in, and the plain resume of the lead: read from
//! the live session file, else the transcript; nothing is written.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use teamwire_core::{Harness, Platform, SessionId};
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
