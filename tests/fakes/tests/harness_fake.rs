//! Unit-level tests of the in-process fake `harness`.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    Address, Harness, HarnessCaps, HarnessError, HarnessErrorCode, IdentityChannel, Liveness,
    MemberKind, MemberLaunch, OtelConfig, Pid, PlaceState, Role, ScreenState, SessionId,
    SessionKind, TeamName, sanitize::one_line,
};
use teamwire_fakes::{ESCALATED_TRUST_TEXT, FakeHarness, PLAIN_TRUST_TEXT};

fn sid(s: &str) -> SessionId {
    SessionId::parse(s).unwrap()
}

fn member() -> MemberLaunch {
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: "/work".into(),
        session_id: sid("s-1"),
        model: None,
        effort: None,
        member_dir: "/home/teams/demo/members/api".into(),
        contract_path: "/home/contract.md".into(),
        identity: vec![
            ("TEAMWIRE_ROLE".into(), "api".into()),
            ("TEAMWIRE_TOKEN".into(), "tok".into()),
        ],
        otel: OtelConfig::default(),
        teamwire_bin: "/bin/teamwire".into(),
        plugins: std::collections::BTreeMap::new(),
        context_handoff_tokens: None,
    }
}

#[test]
fn launch_spec_carries_identity_in_settings_env_when_the_harness_can() {
    let h = FakeHarness::new();
    let spec = h.launch_spec(&member()).unwrap();
    assert_eq!(spec.identity_channel, IdentityChannel::HarnessConfig);
    assert!(spec.env.is_empty());
    let file = spec.files.first().unwrap();
    let doc: Value = serde_json::from_slice(&file.bytes).unwrap();
    assert_eq!(doc.pointer("/env/TEAMWIRE_TOKEN").unwrap(), "tok");
    let stop = doc.pointer("/hooks/Stop/0/hooks/0/command").unwrap();
    assert!(stop.as_str().unwrap().contains("--role api"));
    assert_eq!(file.mode, 0o600);
    assert!(spec.argv.contains(&"--session-id".to_owned()));
}

#[test]
fn without_config_env_identity_moves_to_the_host_channel() {
    let h = FakeHarness::new();
    h.set_caps(HarnessCaps {
        session_env_via_config: false,
        ..FakeHarness::caps()
    });
    let spec = h.launch_spec(&member()).unwrap();
    assert_eq!(spec.identity_channel, IdentityChannel::HostEnv);
    assert_eq!(spec.env.len(), 2);
    assert!(!spec.argv.contains(&"--session-id".to_owned()));
    let doc: Value = serde_json::from_slice(&spec.files.first().unwrap().bytes).unwrap();
    assert!(doc.get("env").is_none());
}

#[test]
fn resume_spec_names_the_session() {
    let spec = FakeHarness::new()
        .resume_spec(&member(), &sid("old-9"))
        .unwrap();
    assert!(spec.argv.windows(2).any(|w| w == ["--resume", "old-9"]));
}

#[test]
fn dead_session_two_processes_and_listing_failure() {
    let h = FakeHarness::new();
    let t = Duration::from_secs(1);
    h.add_session(sid("dead"), vec![], Liveness::Dead);
    h.add_session(
        sid("two"),
        vec![Pid::new(1), Pid::new(2)],
        Liveness::Alive(PlaceState::Idle),
    );
    assert_eq!(h.is_alive(&sid("dead"), t).unwrap(), Liveness::Dead);
    assert_eq!(h.processes_for_session(&sid("two"), t).unwrap().len(), 2);
    assert_eq!(h.is_alive(&sid("nobody"), t).unwrap(), Liveness::Dead);
    h.fail_listing(Some(HarnessError::new(HarnessErrorCode::Timeout, "slow")));
    assert_eq!(
        h.is_alive(&sid("two"), t).unwrap_err().code,
        HarnessErrorCode::Timeout
    );
    assert!(h.processes_for_session(&sid("two"), t).is_err());
    h.fail_listing(None);
    assert!(h.is_alive(&sid("two"), t).is_ok());
}

#[test]
fn background_caller_is_reported() {
    let h = FakeHarness::new();
    assert!(h.caller_session(&|_| None).is_none());
    h.set_background_caller(sid("bg"));
    assert_eq!(
        h.caller_session(&|_| None).unwrap().kind,
        SessionKind::Background
    );
}

#[test]
fn turn_end_parses_the_claude_stop_hook_input() {
    let h = FakeHarness::new();
    let te = h
        .parse_turn_end(br#"{"session_id":"s-1","stop_hook_active":true,"last_assistant_message":"done","transcript_path":"/t.jsonl","agent_id":"a"}"#)
        .unwrap();
    assert_eq!(te.session.as_str(), "s-1");
    assert!(te.hold_already_active && te.subagent);
    assert_eq!(te.final_message.as_deref(), Some("done"));
    assert_eq!(
        h.parse_turn_end(b"nope").unwrap_err().code,
        HarnessErrorCode::BadInput
    );
    assert_eq!(
        h.parse_turn_end(b"{}").unwrap_err().code,
        HarnessErrorCode::BadInput
    );
}

#[test]
fn hold_reply_is_json_carrying_the_note() {
    let reply = FakeHarness::new().hold_reply("report missing");
    let doc: Value = serde_json::from_slice(&reply).unwrap();
    assert_eq!(doc.get("reason").unwrap(), "report missing");
}

#[test]
fn screens_are_classified() {
    let h = FakeHarness::new();
    assert_eq!(
        h.screen_state(&format!("x\n{PLAIN_TRUST_TEXT}\n  No, exit")),
        ScreenState::Dialog
    );
    assert_eq!(h.screen_state(ESCALATED_TRUST_TEXT), ScreenState::Dialog);
    assert_eq!(h.screen_state("ready\n> "), ScreenState::InputPrompt);
    assert_eq!(h.screen_state("compiling"), ScreenState::Unknown);
}

#[test]
fn push_needs_the_capability() {
    let h = FakeHarness::new();
    let line = one_line("hello", 80);
    let t = Duration::from_secs(1);
    assert_eq!(
        h.push(&sid("s"), &line, t).unwrap_err().code,
        teamwire_core::HostErrorCode::Unsupported
    );
    h.set_caps(HarnessCaps {
        can_push: true,
        ..FakeHarness::caps()
    });
    h.push(&sid("s"), &line, t).unwrap();
    assert_eq!(h.pushed(), [(sid("s"), "hello".to_owned())]);
}

#[test]
fn otel_env_is_empty_without_an_endpoint() {
    let h = FakeHarness::new();
    let (team, role) = (TeamName::parse("t").unwrap(), Role::parse("r").unwrap());
    assert!(h.otel_env(&team, &role, &OtelConfig::default()).is_empty());
    let base = OtelConfig {
        endpoint: Some("http://x".into()),
        headers: vec![],
    };
    assert_eq!(h.otel_env(&team, &role, &base).len(), 2);
}

#[test]
fn every_call_is_logged() {
    let h = FakeHarness::new();
    h.screen_state("");
    h.caller_session(&|_| None);
    assert_eq!(h.log().names(), ["screen_state", "caller_session"]);
}
