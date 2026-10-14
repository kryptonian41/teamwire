//! SEC-1: a token never shows in `Debug` output of the records that carry it.

use teamwire_core::*;

const TOKEN: &str = "tok-5f1e9c0d2b7a";

fn env() -> Vec<(String, String)> {
    vec![
        ("TEAMWIRE_TOKEN".to_owned(), TOKEN.to_owned()),
        ("TEAMWIRE_ROLE".to_owned(), "api".to_owned()),
    ]
}

#[test]
fn secret_names() {
    for name in [
        "TEAMWIRE_TOKEN",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "OTEL_API_KEY",
        "my_secret",
    ] {
        assert!(is_secret_name(name), "{name}");
    }
    for name in ["TEAMWIRE_ROLE", "HOME", "PATH"] {
        assert!(!is_secret_name(name), "{name}");
    }
}

#[test]
fn debug_output_hides_the_token() {
    let team = TeamName::parse("tw").unwrap();
    let role = Role::parse("api").unwrap();
    let launch = MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: "/w".into(),
        session_id: SessionId::parse("s1").unwrap(),
        model: None,
        effort: None,
        member_dir: "/h".into(),
        contract_path: "/h/c.md".into(),
        identity: env(),
        otel: OtelConfig {
            endpoint: Some("http://localhost:4318".to_owned()),
            headers: vec![("authorization".to_owned(), TOKEN.to_owned())],
        },
        teamwire_bin: "/bin/teamwire".into(),
        plugins: std::collections::BTreeMap::new(),
    };
    let spec = LaunchSpec {
        argv: vec!["claude".to_owned()],
        env: env(),
        files: vec![LaunchFile {
            path: "settings.json".into(),
            bytes: TOKEN.as_bytes().to_vec(),
            mode: 0o600,
        }],
        identity_channel: IdentityChannel::HarnessConfig,
    };
    let spawn = SpawnRequest {
        name: launch.address.clone(),
        title: "api".to_owned(),
        cwd: "/w".into(),
        argv: Vec::new(),
        env: env(),
    };
    let cmd = Command::new("x").env("TEAMWIRE_TOKEN", TOKEN).stdin(TOKEN);
    for text in [
        format!("{launch:?}"),
        format!("{spec:?}"),
        format!("{spawn:?}"),
        format!("{cmd:?}"),
    ] {
        assert!(!text.contains(TOKEN), "{text}");
        assert!(
            text.contains("TEAMWIRE_TOKEN=<redacted>")
                || text.contains("<5 bytes>")
                || text.contains("bytes"),
            "{text}"
        );
    }
    assert!(format!("{launch:?}").contains("TEAMWIRE_ROLE=api"));
}
