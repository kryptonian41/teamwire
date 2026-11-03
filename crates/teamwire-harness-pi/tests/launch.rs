//! `launch_spec` and `resume_spec`: the argv, the token file and the
//! extension file.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes the contract file launch would, not through the platform"
)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use teamwire_core::{
    Address, Harness, HarnessErrorCode, IdentityChannel, LaunchSpec, MemberKind, MemberLaunch,
    OtelConfig, Role, SessionId, TeamName,
};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

const TOKEN: &str = "tok-0123456789abcdef";
const SESSION: &str = "01a1725a-5528-728a-b86b-775340828a01";

fn pi() -> Pi {
    Pi::new(
        Arc::new(FakePlatform::new()),
        PiConfig {
            agent_dir: "/nowhere/pi".into(),
            pi_bin: "pi".into(),
            exclude_providers: Vec::new(),
        },
    )
}

/// A member whose home is `<root>/home`, with a contract file on disk.
fn member(root: &Path) -> MemberLaunch {
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    let contract = root.join("home/teams/demo/contract/api.md");
    std::fs::create_dir_all(contract.parent().unwrap()).unwrap();
    std::fs::write(&contract, "Report done.\n").unwrap();
    MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: "/work".into(),
        session_id: SessionId::parse(SESSION).unwrap(),
        model: None,
        effort: None,
        member_dir: root.join("home/teams/demo/members/api"),
        contract_path: contract,
        identity: vec![
            ("TEAMWIRE_TEAM".into(), "demo".into()),
            ("TEAMWIRE_ROLE".into(), "api".into()),
            ("TEAMWIRE_TOKEN".into(), TOKEN.into()),
        ],
        otel: OtelConfig::default(),
        teamwire_bin: "/usr/local/bin/teamwire".into(),
        plugins: BTreeMap::new(),
        context_handoff_tokens: None,
        accept_trust: false,
    }
}

/// The value after `flag` in argv.
fn after<'a>(spec: &'a LaunchSpec, flag: &str) -> Option<&'a str> {
    let at = spec.argv.iter().position(|a| a == flag)?;
    spec.argv.get(at + 1).map(String::as_str)
}

fn file<'a>(spec: &'a LaunchSpec, name: &str) -> &'a teamwire_core::LaunchFile {
    spec.files
        .iter()
        .find(|f| f.path.file_name().is_some_and(|n| n == name))
        .unwrap()
}

#[test]
fn a_start_names_the_session_its_folder_and_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    let spec = pi().launch_spec(&m).unwrap();
    assert_eq!(spec.argv.first().map(String::as_str), Some("pi"));
    assert_eq!(after(&spec, "--session-id"), Some(SESSION));
    assert_eq!(
        after(&spec, "--session-dir"),
        Some("/nowhere/pi/sessions/teamwire")
    );
    assert!(spec.argv.iter().any(|a| a == "--no-approve"));
    let extension = m.member_dir.join("pi-extension.ts");
    assert_eq!(
        after(&spec, "--extension"),
        Some(extension.display().to_string().as_str())
    );
    assert_eq!(
        after(&spec, "--append-system-prompt"),
        Some(m.contract_path.display().to_string().as_str())
    );
    assert_eq!(spec.identity_channel, IdentityChannel::HarnessConfig);
    assert!(spec.env.is_empty());
}

#[test]
fn the_token_goes_only_into_its_private_file() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    let spec = pi().launch_spec(&m).unwrap();
    let token = file(&spec, "token");
    assert_eq!(token.bytes, TOKEN.as_bytes());
    assert_eq!(token.mode, 0o600);
    assert!(!spec.argv.iter().any(|a| a.contains(TOKEN)));

    let ext = String::from_utf8(file(&spec, "pi-extension.ts").bytes.clone()).unwrap();
    assert!(!ext.contains(TOKEN));
    let token_path = m.member_dir.join("token").display().to_string();
    assert!(ext.contains(&format!("\"TEAMWIRE_TOKEN_FILE\":\"{token_path}\"")));
    let home = dir.path().join("home").display().to_string();
    assert!(ext.contains(&format!("\"homeArgs\":[\"--home\",\"{home}\"]")));
    assert!(ext.contains("\"bin\":\"/usr/local/bin/teamwire\""));
}

#[test]
fn model_and_effort_become_pis_flags() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    let spec = pi().launch_spec(&m).unwrap();
    assert_eq!(after(&spec, "--model"), None);
    assert_eq!(after(&spec, "--thinking"), None);
    m.model = Some("anthropic/claude-sonnet-4-5".into());
    m.effort = Some("high".into());
    let spec = pi().launch_spec(&m).unwrap();
    assert_eq!(after(&spec, "--model"), Some("anthropic/claude-sonnet-4-5"));
    assert_eq!(after(&spec, "--thinking"), Some("high"));
}

#[test]
fn a_resume_is_the_same_start_for_the_named_session() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    let other = SessionId::parse("22222222-2222-4222-8222-222222222222").unwrap();
    let resumed = pi().resume_spec(&m, &other).unwrap();
    assert_eq!(after(&resumed, "--session-id"), Some(other.as_str()));
    let started = pi().launch_spec(&m).unwrap();
    let without_id = |spec: &LaunchSpec| {
        let mut argv = spec.argv.clone();
        let at = argv.iter().position(|a| a == "--session-id").unwrap();
        argv.drain(at..=at + 1);
        argv
    };
    assert_eq!(without_id(&resumed), without_id(&started));
}

#[test]
fn a_member_without_a_token_or_a_contract_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.identity.retain(|(k, _)| k != "TEAMWIRE_TOKEN");
    let err = pi().launch_spec(&m).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Refused);

    let mut m = member(dir.path());
    m.contract_path = dir.path().join("missing.md");
    let err = pi().launch_spec(&m).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Io);
}
