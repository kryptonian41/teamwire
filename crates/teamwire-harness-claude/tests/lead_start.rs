//! A fresh lead's command line (ADR 0051): the new session's id, the old
//! lead's allowed flags, the permission mode and model of the old
//! transcript, and the first prompt last. Nothing is written.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use teamwire_core::{Harness, HarnessErrorCode, LeadStart, Platform, SessionId};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

const OLD: &str = "0f2c7b1e-6a4d-4e8b-9c3f-1a2b3c4d5e6f";
const NEW: &str = "7d9e1f2a-3b4c-4d5e-8f6a-7b8c9d0e1f2a";
const PROMPT: &str = "You are a teamwire lead. Take over.";

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

fn request(old_args: &str) -> LeadStart {
    LeadStart {
        old_session: SessionId::parse(OLD).unwrap(),
        old_args: old_args.split(' ').map(str::to_owned).collect(),
        new_session: Some(SessionId::parse(NEW).unwrap()),
        prompt: PROMPT.to_owned(),
    }
}

fn words(text: &str) -> Vec<String> {
    text.split(' ').map(str::to_owned).collect()
}

/// A user line and an assistant line with the keys a real Claude Code
/// transcript of this machine carries (2026-11-17).
fn user(mode: &str) -> Value {
    json!({
        "type": "user", "isSidechain": false, "userType": "external",
        "sessionId": OLD, "permissionMode": mode,
        "message": { "role": "user", "content": "go on" },
    })
}

fn assistant(model: &str, sidechain: bool) -> Value {
    json!({
        "type": "assistant", "isSidechain": sidechain, "sessionId": OLD,
        "requestedModel": model, "effort": "high",
        "message": {
            "role": "assistant", "model": model, "type": "message",
            "usage": { "input_tokens": 5, "cache_read_input_tokens": 400_000 },
        },
    })
}

fn transcript(dir: &Path, lines: &[Value]) {
    let file = dir.join("projects/-work-lead").join(format!("{OLD}.jsonl"));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut text = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    FakePlatform::new()
        .write_atomic(&file, text.as_bytes())
        .unwrap();
}

#[test]
fn the_new_lead_gets_its_own_session_the_allowed_flags_and_the_prompt_last() {
    let dir = tempfile::tempdir().unwrap();
    let old = format!(
        "/opt/bin/claude --dangerously-skip-permissions --model opus --effort=xhigh --verbose \
         --resume {OLD} --add-dir /x --settings {{\"env\":{{\"TOKEN\":\"s3cret\"}}}} -c -r --ide -- old prompt"
    );
    // `-r` takes an optional value, so the flag after it is no value.
    let spec = claude(dir.path()).lead_start(&request(&old)).unwrap();
    assert_eq!(
        spec.argv,
        words(&format!(
            "claude --session-id {NEW} --dangerously-skip-permissions --model opus --effort=xhigh --verbose --ide"
        ))
        .into_iter()
        .chain([PROMPT.to_owned()])
        .collect::<Vec<_>>()
    );
    assert_eq!(spec.dropped, words("--add-dir --settings"));
    let all = format!("{:?}", (&spec.argv, &spec.dropped));
    assert!(!all.contains("s3cret"), "{all}");
    assert!(!all.contains(OLD), "{all}");
}

#[test]
fn the_transcript_wins_for_the_permission_mode_and_the_model() {
    let dir = tempfile::tempdir().unwrap();
    // The newest main-chain model is opus. A newer subagent line and two
    // newer API error lines, one marked and one with Claude Code's
    // `<synthetic>` model, do not count.
    let mut marked = assistant("claude-sonnet-5-5", false);
    marked
        .as_object_mut()
        .unwrap()
        .insert("isApiErrorMessage".to_owned(), json!(true));
    transcript(
        dir.path(),
        &[
            user("bypassPermissions"),
            assistant("claude-opus-5-5", false),
            assistant("claude-haiku-5-5", true),
            marked,
            assistant("<synthetic>", false),
        ],
    );
    let spec = claude(dir.path())
        .lead_start(&request("claude --dangerously-skip-permissions"))
        .unwrap();
    assert_eq!(
        spec.argv,
        words(&format!(
            "claude --session-id {NEW} --permission-mode bypassPermissions --model claude-opus-5-5"
        ))
        .into_iter()
        .chain([PROMPT.to_owned()])
        .collect::<Vec<_>>()
    );
}

#[test]
fn a_mode_changed_away_from_bypass_keeps_the_right_to_switch_back() {
    let dir = tempfile::tempdir().unwrap();
    transcript(
        dir.path(),
        &[
            user("bypassPermissions"),
            assistant("claude-opus-5-5", false),
            user("acceptEdits"),
        ],
    );
    let spec = claude(dir.path())
        .lead_start(&request(
            "claude --dangerously-skip-permissions --permission-mode=auto --model claude-opus-5-5[1m]",
        ))
        .unwrap();
    // The old model value starts with the transcript's, so its `[1m]` stays.
    assert_eq!(
        spec.argv,
        words(&format!(
            "claude --session-id {NEW} --model claude-opus-5-5[1m] \
             --allow-dangerously-skip-permissions --permission-mode acceptEdits"
        ))
        .into_iter()
        .chain([PROMPT.to_owned()])
        .collect::<Vec<_>>()
    );
}

#[test]
fn a_mode_claude_does_not_accept_leaves_the_old_flags() {
    let dir = tempfile::tempdir().unwrap();
    transcript(dir.path(), &[user("default")]);
    let spec = claude(dir.path())
        .lead_start(&request("claude --dangerously-skip-permissions"))
        .unwrap();
    assert_eq!(
        spec.argv,
        [
            "claude",
            "--session-id",
            NEW,
            "--dangerously-skip-permissions",
            PROMPT
        ]
    );
}

#[test]
fn a_lead_start_without_a_new_session_id_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request("claude");
    req.new_session = None;
    let err = claude(dir.path()).lead_start(&req).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::BadInput);
}
