//! The hook input the pi extension writes, and the hold reply it reads.

#![expect(
    clippy::indexing_slicing,
    reason = "tests read the JSON by key; a missing key reads as null and fails the assert"
)]

use std::sync::Arc;

use teamwire_core::{Harness, HarnessErrorCode};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

fn pi() -> Pi {
    Pi::new(
        Arc::new(FakePlatform::new()),
        PiConfig {
            agent_dir: "/nowhere/pi".into(),
            pi_bin: "pi".into(),
        },
    )
}

#[test]
fn the_turn_end_input_maps_to_the_neutral_form() {
    let input = br#"{"session_id":"s-1","hold_active":true,"final_message":"All done.","session_file":"/p/s.jsonl"}"#;
    let end = pi().parse_turn_end(input).unwrap();
    assert_eq!(end.session.as_str(), "s-1");
    assert!(end.hold_already_active);
    assert!(!end.subagent);
    assert_eq!(end.final_message.as_deref(), Some("All done."));
    assert_eq!(end.transcript.as_deref(), Some("/p/s.jsonl".as_ref()));
}

#[test]
fn the_post_tool_input_needs_only_the_session() {
    let end = pi().parse_turn_end(br#"{"session_id":"s-1"}"#).unwrap();
    assert!(!end.hold_already_active);
    assert_eq!(end.final_message, None);
    assert_eq!(end.transcript, None);
}

#[test]
fn an_input_without_a_session_is_bad_input() {
    for input in [&br#"{"session_id":"  "}"#[..], b"[1]", b"{}"] {
        let err = pi().parse_turn_end(input).unwrap_err();
        assert_eq!(err.code, HarnessErrorCode::BadInput);
    }
}

#[test]
fn the_hold_reply_is_the_block_the_extension_reads() {
    let reply: serde_json::Value =
        serde_json::from_slice(&pi().hold_reply("send \"progress\" first")).unwrap();
    assert_eq!(reply["decision"], "block");
    assert_eq!(reply["reason"], "send \"progress\" first");
}
