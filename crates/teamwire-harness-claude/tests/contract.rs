//! The shared harness contract suite, run against `ClaudeCode` with its
//! paths in a temporary directory and a live `fake-claude` session.

#[path = "support/env_platform.rs"]
mod env_platform;

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Address, DialogKind, MemberKind, MemberLaunch, OtelConfig, Role, SessionId, TeamName,
};
use teamwire_fakes::contract::{
    HarnessSetup, PermissionSetup, harness_contract, permission_contract,
};
use teamwire_fakes::{FakeHerdrEnv, PLAIN_TRUST_TEXT};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

use env_platform::EnvPlatform;

#[test]
fn claude_code_passes_the_harness_contract() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&teamwire_fakes::ClaudeScript::default())
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
            "contract-live",
        ])
        .unwrap();
    assert_eq!(started.code, Some(0), "{started:?}");

    let claude = ClaudeCode::new(
        Arc::new(EnvPlatform::new(env.env())),
        ClaudeConfig {
            claude_dir: env.claude_dir(),
            claude_json: env.root().join("home/.claude.json"),
            claude_bin: "claude".to_owned(),
        },
    );
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    let member_dir = env.root().join("teams/demo/members/api");
    let member = MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: env.root().to_path_buf(),
        session_id: SessionId::parse("6f1c2d3e-0000-4000-8000-000000000001").unwrap(),
        model: None,
        effort: None,
        contract_path: env.root().join("teams/demo/contract/api.md"),
        member_dir,
        identity: vec![
            ("TEAMWIRE_TEAM".into(), "demo".into()),
            ("TEAMWIRE_ROLE".into(), "api".into()),
            ("TEAMWIRE_TOKEN".into(), "tok-contract-secret".into()),
        ],
        otel: OtelConfig::default(),
        teamwire_bin: "/usr/local/bin/teamwire".into(),
        plugins: std::collections::BTreeMap::new(),
        context_handoff_tokens: None,
        accept_trust: false,
    };
    harness_contract(&HarnessSetup {
        harness: &claude,
        member,
        turn_end_stdin: br#"{"session_id":"member-1","stop_hook_active":false}"#.to_vec(),
        turn_end_session: SessionId::parse("member-1").unwrap(),
        prompt_screen: "ready\n❯ \n  ? for shortcuts".into(),
        dialog_screen: PLAIN_TRUST_TEXT.into(),
        live_session: Some(SessionId::parse("contract-live").unwrap()),
        unknown_session: SessionId::parse("never-heard-of").unwrap(),
        timeout: Duration::from_secs(5),
    });
}

#[expect(clippy::unwrap_used, reason = "a missing fixture fails the test")]
fn permission_fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/permission/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap()
}

#[test]
fn claude_code_passes_the_permission_contract() {
    let claude = ClaudeCode::new(
        Arc::new(teamwire_fakes::FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nonexistent/.claude".into(),
            claude_json: "/nonexistent/.claude.json".into(),
            claude_bin: "claude".to_owned(),
        },
    );
    let mut other: serde_json::Value =
        serde_json::from_str(&permission_fixture("bash-rmdir.json")).unwrap();
    other
        .as_object_mut()
        .unwrap()
        .insert("tool_name".into(), serde_json::json!("AskUserQuestion"));
    permission_contract(&PermissionSetup {
        harness: Box::new(claude),
        prompt_screen: permission_fixture("input-prompt.txt"),
        named_screens: vec![
            (
                permission_fixture("bash-rmdir-countdown.txt"),
                DialogKind::Permission,
            ),
            (permission_fixture("webfetch.txt"), DialogKind::Permission),
            (PLAIN_TRUST_TEXT.to_owned(), DialogKind::Trust),
        ],
        answerable_stdin: Some(permission_fixture("bash-rmdir.json").into_bytes()),
        other_tool_stdin: Some(other.to_string().into_bytes()),
    });
}
