//! The shared harness contract suite against `Pi`, with a live session
//! that has had no message yet: only the extension's state file exists. The member's identity
//! holds the team, role and token: the first two travel in the extension
//! file and the token in its own 0600 file, and the suite wants each value
//! in a launch file.

#![expect(
    clippy::disallowed_methods,
    reason = "the test writes the state file as the extension would, not through the platform"
)]

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Address, MemberKind, MemberLaunch, OtelConfig, Pid, Role, SessionId, TeamName, Timestamp,
};
use teamwire_fakes::FakePlatform;
use teamwire_fakes::contract::{HarnessSetup, harness_contract};
use teamwire_harness_pi::{Pi, PiConfig};

#[test]
fn pi_passes_the_harness_contract() {
    let dir = tempfile::tempdir().unwrap();
    let live = "019e0000-0000-7000-8000-0000000000aa";
    let sessions = dir.path().join("agent/sessions/teamwire");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join(format!("{live}.teamwire.json")),
        r#"{"pid":77,"started_at_ms":1790000000000,"state":"idle"}"#,
    )
    .unwrap();
    let platform = Arc::new(FakePlatform::new());
    platform.set_alive(Pid::new(77), true);
    platform.set_started(Pid::new(77), Timestamp::from_unix_millis(1_790_000_000_000));
    let pi = Pi::new(
        platform,
        PiConfig {
            agent_dir: dir.path().join("agent"),
            pi_bin: "pi".into(),
        },
    );
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    let contract = dir.path().join("contract.md");
    std::fs::write(&contract, "be good").unwrap();
    let member = MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: dir.path().to_path_buf(),
        session_id: SessionId::parse("019e0000-0000-7000-8000-000000000001").unwrap(),
        model: None,
        effort: None,
        member_dir: dir.path().join("home/teams/demo/members/api"),
        contract_path: contract,
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
        harness: &pi,
        member,
        turn_end_stdin: br#"{"session_id":"member-1","hold_active":false}"#.to_vec(),
        turn_end_session: SessionId::parse("member-1").unwrap(),
        prompt_screen: include_str!("screens/prompt.txt").into(),
        dialog_screen: include_str!("screens/trust.txt").into(),
        live_session: Some(SessionId::parse(live).unwrap()),
        unknown_session: SessionId::parse("never-heard-of").unwrap(),
        timeout: Duration::from_secs(5),
    });
}
