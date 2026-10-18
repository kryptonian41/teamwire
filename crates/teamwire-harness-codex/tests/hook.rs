//! Codex hook input and the hold reply.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::sync::Arc;

use serde_json::{Value, json};
use teamwire_core::{Harness, HarnessErrorCode, SessionId};
use teamwire_fakes::FakePlatform;
use teamwire_harness_codex::{CodexCli, CodexConfig};

const SESSION: &str = "019e0000-0000-7000-8000-000000000001";

fn codex() -> CodexCli {
    CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    )
}

fn parse(doc: &Value) -> teamwire_core::TurnEnd {
    codex().parse_turn_end(doc.to_string().as_bytes()).unwrap()
}

#[test]
fn a_stop_input_maps_field_by_field() {
    // Every field of hooks/schema/generated/stop.command.input.schema.json.
    let end = parse(&json!({
        "cwd": "/work",
        "hook_event_name": "Stop",
        "last_assistant_message": "All done.",
        "model": "gpt-5.5",
        "permission_mode": "bypassPermissions",
        "session_id": SESSION,
        "stop_hook_active": true,
        "transcript_path": "/home/u/.codex/sessions/2026/10/18/rollout-x.jsonl",
        "turn_id": "t1",
    }));
    assert_eq!(end.session, SessionId::parse(SESSION).unwrap());
    assert!(!end.subagent);
    assert!(end.hold_already_active);
    assert_eq!(end.final_message.as_deref(), Some("All done."));
    assert_eq!(
        end.transcript.unwrap().to_str(),
        Some("/home/u/.codex/sessions/2026/10/18/rollout-x.jsonl")
    );
}

#[test]
fn null_fields_are_absent_and_only_the_session_is_required() {
    let end = parse(&json!({
        "session_id": SESSION,
        "last_assistant_message": null,
        "transcript_path": null,
    }));
    assert_eq!(end.final_message, None);
    assert_eq!(end.transcript, None);
    assert!(!end.hold_already_active);
}

#[test]
fn post_tool_use_and_session_start_inputs_parse_too() {
    let post = parse(&json!({
        "hook_event_name": "PostToolUse", "session_id": SESSION, "tool_name": "Bash",
        "tool_input": {"command": "ls"}, "tool_response": "x", "tool_use_id": "u",
        "transcript_path": null, "turn_id": "t", "cwd": "/w", "model": "m",
        "permission_mode": "default",
    }));
    assert!(!post.subagent);
    let sub = parse(&json!({"session_id": SESSION, "agent_id": "a1", "agent_type": "worker"}));
    assert!(sub.subagent);
    let start = parse(&json!({
        "hook_event_name": "SessionStart", "session_id": SESSION, "source": "startup",
        "transcript_path": "/t.jsonl", "cwd": "/w", "model": "m", "permission_mode": "default",
    }));
    assert_eq!(start.session.as_str(), SESSION);
}

#[test]
fn junk_input_is_bad_input() {
    for junk in [
        &b"not json"[..],
        b"",
        b"[1]",
        b"{}",
        br#"{"session_id":""}"#,
        br#"{"session_id":7}"#,
    ] {
        let err = codex().parse_turn_end(junk).unwrap_err();
        assert_eq!(err.code, HarnessErrorCode::BadInput, "{junk:?}");
    }
}

#[test]
fn the_hold_reply_blocks_with_the_note_as_reason() {
    let note = "say \"hi\"\nthen report";
    let reply: Value = serde_json::from_slice(&codex().hold_reply(note)).unwrap();
    assert_eq!(reply, json!({"decision": "block", "reason": note}));
}
