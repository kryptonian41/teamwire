//! The Stop hook's input and the hold reply, including a hold that
//! `fake-claude` honours.

#![expect(
    clippy::indexing_slicing,
    reason = "tests build fixtures from literals and edit JSON by key; a missing key should fail the test"
)]

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{Harness, HarnessErrorCode, SessionId};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, FakePlatform, Step};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn claude() -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nowhere".into(),
            claude_json: "/nowhere/claude.json".into(),
            claude_bin: "claude".into(),
        },
    )
}

fn parse(doc: &Value) -> Result<teamwire_core::TurnEnd, teamwire_core::HarnessError> {
    claude().parse_turn_end(doc.to_string().as_bytes())
}

/// The keys a real Claude Code 2.1.284 Stop input carries.
fn stop_input() -> Value {
    json!({
        "session_id": "6f1c2d3e-0000-4000-8000-000000000001",
        "transcript_path": "/home/u/.claude/projects/x/6f1c.jsonl",
        "cwd": "/work",
        "permission_mode": "bypassPermissions",
        "hook_event_name": "Stop",
        "stop_hook_active": false,
        "prompt_id": "p-1",
        "background_tasks": [],
        "last_assistant_message": "All done."
    })
}

#[test]
fn a_real_stop_input_is_parsed() {
    let end = parse(&stop_input()).unwrap();
    assert_eq!(
        end.session,
        SessionId::parse("6f1c2d3e-0000-4000-8000-000000000001").unwrap()
    );
    assert!(!end.subagent);
    assert!(!end.hold_already_active);
    assert_eq!(end.final_message.as_deref(), Some("All done."));
    assert_eq!(
        end.transcript.as_deref(),
        Some(std::path::Path::new(
            "/home/u/.claude/projects/x/6f1c.jsonl"
        ))
    );
}

#[test]
fn stop_hook_active_is_carried() {
    let mut doc = stop_input();
    doc["stop_hook_active"] = json!(true);
    assert!(parse(&doc).unwrap().hold_already_active);
}

#[test]
fn an_agent_id_marks_a_subagent_stop() {
    let mut doc = stop_input();
    doc["agent_id"] = json!("agent-7");
    assert!(parse(&doc).unwrap().subagent);
}

#[test]
fn a_null_or_empty_agent_id_is_not_a_subagent() {
    for value in [json!(null), json!("")] {
        let mut doc = stop_input();
        doc["agent_id"] = value;
        assert!(!parse(&doc).unwrap().subagent);
    }
}

#[test]
fn a_paused_session_with_background_tasks_reports_nothing() {
    let mut doc = stop_input();
    doc["background_tasks"] = json!([{ "id": "b1", "status": "running" }]);
    assert!(parse(&doc).unwrap().subagent);
}

#[test]
fn the_last_message_and_transcript_are_optional() {
    let mut doc = stop_input();
    let map = doc.as_object_mut().unwrap();
    map.remove("last_assistant_message");
    map.remove("transcript_path");
    map.remove("stop_hook_active");
    map.remove("background_tasks");
    let end = parse(&doc).unwrap();
    assert_eq!(end.final_message, None);
    assert_eq!(end.transcript, None);
    assert!(!end.hold_already_active);
}

#[test]
fn a_blank_last_message_is_none() {
    let mut doc = stop_input();
    doc["last_assistant_message"] = json!("  \n");
    assert_eq!(parse(&doc).unwrap().final_message, None);
}

#[test]
fn input_without_a_usable_session_id_is_bad_input() {
    let mut no_key = stop_input();
    no_key.as_object_mut().unwrap().remove("session_id");
    let mut blank = stop_input();
    blank["session_id"] = json!("");
    let mut number = stop_input();
    number["session_id"] = json!(7);
    for doc in [no_key, blank, number] {
        assert_eq!(
            parse(&doc).unwrap_err().code,
            HarnessErrorCode::BadInput,
            "{doc}"
        );
    }
}

#[test]
fn junk_is_bad_input() {
    for junk in [&b""[..], b"not json", b"[]", b"7", b"null"] {
        let err = claude().parse_turn_end(junk).unwrap_err();
        assert_eq!(err.code, HarnessErrorCode::BadInput);
    }
}

#[test]
fn the_hold_reply_is_a_block_decision_with_the_note() {
    let note = "add a progress line for abc1234\nthen stop again \"once\"";
    let reply: Value = serde_json::from_slice(&claude().hold_reply(note)).unwrap();
    assert_eq!(reply, json!({ "decision": "block", "reason": note }));
}

/// Polls for at most about 20 s. A count of sleeps stands in for a clock, which
/// tests may not read.
fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..800 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(done(), "{what} never happened");
}

/// `fake-claude` runs a Stop hook that prints the adapter's own hold reply
/// once. The fake must hold the turn (showing the note as feedback, running
/// the hold steps), stop again with `stop_hook_active`, and hand the
/// adapter's parser both real inputs.
#[test]
fn fake_claude_honours_the_hold_reply_and_the_adapter_parses_its_inputs() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        on_hold: vec![Step::Run(r#"echo added > "$TMPDIR/hold-step""#.into())],
        ..ClaudeScript::default()
    })
    .unwrap();
    let note = "add your progress line";
    let reply = String::from_utf8(claude().hold_reply(note)).unwrap();
    env.write_file("tmp/hold-reply.json", &reply).unwrap();
    let hook = r#"cat >> "$TMPDIR/inputs.jsonl"; printf '\n' >> "$TMPDIR/inputs.jsonl"; if [ ! -f "$TMPDIR/held" ]; then touch "$TMPDIR/held"; cat "$TMPDIR/hold-reply.json"; fi"#;
    let settings = env
        .write_file(
            "settings.json",
            &json!({
                "env": { "TEAMWIRE_ROLE": "api" },
                "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": hook }] }] },
            })
            .to_string(),
        )
        .unwrap();
    let pane = env
        .run(&["workspace", "create", "--cwd", "/tmp", "--no-focus"])
        .unwrap()
        .result()
        .unwrap()
        .pointer("/root_pane/pane_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let started = env
        .run(&[
            "agent",
            "start",
            "tw_demo_api",
            "--kind",
            "claude",
            "--pane",
            &pane,
            "--timeout",
            "20000",
            "--",
            "-n",
            "tw_demo_api",
            "--session-id",
            "sess-hold",
            "--settings",
            settings.to_str().unwrap(),
        ])
        .unwrap();
    assert_eq!(started.code, Some(0), "{started:?}");
    let out = env
        .run(&[
            "agent",
            "prompt",
            "tw_demo_api",
            "finish",
            "--wait",
            "--timeout",
            "20000",
        ])
        .unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");

    let inputs_path = env.root().join("tmp/inputs.jsonl");
    wait_for("both Stop inputs", || {
        std::fs::read_to_string(&inputs_path)
            .is_ok_and(|t| t.lines().filter(|l| !l.trim().is_empty()).count() >= 2)
    });
    let text = std::fs::read_to_string(&inputs_path).unwrap();
    let parsed: Vec<_> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| claude().parse_turn_end(l.as_bytes()).unwrap())
        .collect();
    assert_eq!(
        parsed
            .iter()
            .map(|e| e.hold_already_active)
            .collect::<Vec<_>>(),
        [false, true]
    );
    assert!(
        parsed
            .iter()
            .all(|e| e.session.as_str() == "sess-hold" && !e.subagent)
    );
    assert!(
        env.root().join("tmp/hold-step").is_file(),
        "the hold steps ran"
    );
    let screen = env
        .run(&["agent", "read", "tw_demo_api", "--lines", "30"])
        .unwrap();
    let screen = screen.result().unwrap();
    let screen = screen.pointer("/read/text").unwrap().as_str().unwrap();
    assert!(
        screen.contains(&format!("Stop hook feedback: {note}")),
        "{screen}"
    );
}
