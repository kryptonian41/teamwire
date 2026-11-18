//! Claude Code's `PermissionRequest` hook (ADR 0052): the hook input it
//! sends, the reply teamwire prints, and the dialog kind its screen shows.
//! The inputs and screens under `fixtures/permission/` are captured from
//! real Claude Code 2.1.327 sessions; only home-folder paths were replaced.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests build fixtures from literals and read JSON by key; a missing key should fail the test"
)]

use std::sync::Arc;

use serde_json::{Value, json};
use teamwire_core::{
    DialogKind, DialogVerdict, Harness, HarnessErrorCode, PermissionAnswer, PermissionDecision,
    SUMMARY_MAX, SessionId, Timestamp, Ulid, decision_for,
};
use teamwire_fakes::{FakePlatform, PLAIN_TRUST_TEXT};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn claude() -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nonexistent/.claude".into(),
            claude_json: "/nonexistent/.claude.json".into(),
            claude_bin: "claude".to_owned(),
        },
    )
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/permission/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap()
}

/// `bash-rmdir.json` with `edit` applied to its JSON.
fn edited(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut doc: Value = serde_json::from_str(&fixture("bash-rmdir.json")).unwrap();
    edit(&mut doc);
    doc.to_string().into_bytes()
}

#[test]
fn each_captured_input_parses_with_its_summary() {
    let cases = [
        ("bash-rmdir.json", "Bash", "rmdir /usr"),
        ("bash-ask-rule.json", "Bash", "echo askme hello"),
        (
            "write.json",
            "Write",
            "Write /work/exp/work/new.txt (5 bytes)",
        ),
        ("edit.json", "Edit", "Edit /work/exp/work/edit-me.txt"),
        ("webfetch.json", "WebFetch", "WebFetch https://example.com"),
    ];
    for (file, tool, summary) in cases {
        let text = fixture(file);
        let req = claude().parse_permission_request(text.as_bytes()).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(req.tool, tool, "{file}");
        assert_eq!(req.summary, summary, "{file}");
        assert_eq!(req.why_not, None, "{file}");
        assert!(req.answerable(), "{file}");
        assert!(!req.subagent, "{file}");
        assert_eq!(
            req.session,
            SessionId::parse(doc["session_id"].as_str().unwrap()).unwrap()
        );
        assert_eq!(req.input.as_ref(), Some(&doc["tool_input"]), "{file}");
    }
}

#[test]
fn a_tool_teamwire_never_answers_is_kept_but_not_answerable() {
    let stdin = edited(|d| {
        d["tool_name"] = json!("AskUserQuestion");
        d["tool_input"] = json!({"questions": [{"question": "Which?"}]});
    });
    let req = claude().parse_permission_request(&stdin).unwrap();
    assert_eq!(req.tool, "AskUserQuestion");
    assert_eq!(req.summary, "AskUserQuestion");
    assert_eq!(
        req.why_not.as_deref(),
        Some("teamwire answers only Bash, Write, Edit and WebFetch calls")
    );
    assert!(!req.answerable());
}

#[test]
fn an_input_over_64_kib_is_not_answerable_and_not_kept_whole() {
    let command = format!("echo {}", "x".repeat(70_000));
    let stdin = edited(|d| d["tool_input"] = json!({ "command": command }));
    let req = claude().parse_permission_request(&stdin).unwrap();
    let bytes = serde_json::to_vec(&json!({ "command": command }))
        .unwrap()
        .len();
    assert_eq!(req.input, None);
    assert_eq!(
        req.why_not.unwrap(),
        format!("the input is {bytes} bytes, over the 64 KiB teamwire shows whole")
    );
    assert_eq!(req.summary.chars().count(), SUMMARY_MAX);
    assert!(command.starts_with(&req.summary));
}

#[test]
fn a_known_tool_with_its_key_missing_is_not_answerable() {
    for (tool, input, why) in [
        (
            "Bash",
            json!({"command": ""}),
            "the Bash input has no command",
        ),
        (
            "Write",
            json!({"content": "x"}),
            "the Write input has no file_path",
        ),
        ("Edit", json!({}), "the Edit input has no file_path"),
        (
            "WebFetch",
            json!({"url": "  "}),
            "the WebFetch input has no url",
        ),
    ] {
        let stdin = edited(|d| {
            d["tool_name"] = json!(tool);
            d["tool_input"] = input;
        });
        let req = claude().parse_permission_request(&stdin).unwrap();
        assert_eq!(req.why_not.as_deref(), Some(why), "{tool}");
    }
}

#[test]
fn a_subagent_call_is_marked() {
    let stdin = edited(|d| d["agent_id"] = json!("a1b2"));
    assert!(claude().parse_permission_request(&stdin).unwrap().subagent);
}

#[test]
fn input_that_is_not_a_permission_request_is_bad_input() {
    let cases = [
        b"not json".to_vec(),
        edited(|d| d["hook_event_name"] = json!("PreToolUse")),
        edited(|d| d["session_id"] = json!("")),
        edited(|d| d["tool_name"] = json!(7)),
        edited(|d| d["tool_input"] = json!("rmdir /usr")),
    ];
    for stdin in cases {
        let got = claude().parse_permission_request(&stdin);
        assert!(
            matches!(&got, Err(e) if e.code == HarnessErrorCode::BadInput),
            "{}: {got:?}",
            String::from_utf8_lossy(&stdin)
        );
    }
}

#[test]
fn the_replies_allow_once_or_deny_with_the_message_and_nothing_more() {
    let approve = PermissionDecision {
        verdict: DialogVerdict::Approve,
        message: None,
    };
    let bytes = claude().permission_reply(&approve).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
    );

    let req = claude()
        .parse_permission_request(fixture("bash-rmdir.json").as_bytes())
        .unwrap();
    let id = Ulid::from_parts(Timestamp::from_unix_millis(1_790_000_000_000).unwrap(), 3);
    let deny = decision_for(
        &id,
        &req,
        &PermissionAnswer {
            verdict: DialogVerdict::Deny,
            reason: "a system folder \"quoted\"".to_owned(),
        },
    );
    let bytes = claude().permission_reply(&deny).unwrap();
    let reply: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        reply,
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest",
               "decision": {"behavior": "deny", "message": deny.message.unwrap()}}})
    );
}

#[test]
fn the_dialog_kind_of_each_captured_screen() {
    let cases = [
        ("bash-rmdir-countdown.txt", DialogKind::Permission),
        ("bash-rmdir-hook-waiting.txt", DialogKind::Permission),
        ("bash-ask-rule.txt", DialogKind::Permission),
        ("write.txt", DialogKind::Permission),
        ("edit.txt", DialogKind::Permission),
        ("webfetch.txt", DialogKind::Permission),
        ("input-prompt.txt", DialogKind::NoDialog),
        ("after-hook-allow.txt", DialogKind::NoDialog),
        ("after-hook-deny.txt", DialogKind::NoDialog),
        ("after-silent-timeout.txt", DialogKind::NoDialog),
    ];
    for (file, want) in cases {
        assert_eq!(claude().dialog_kind(&fixture(file)), want, "{file}");
    }
    assert_eq!(claude().dialog_kind(PLAIN_TRUST_TEXT), DialogKind::Trust);
    // A dialog with neither a trust nor a permission marker.
    assert_eq!(
        claude().dialog_kind("Some new dialog\n \u{276f} 1. Go on\n   2. Stop\n\n Esc to cancel"),
        DialogKind::Unknown
    );
}
