//! `teamwire teams` and `teamwire send --to role@team` through the real
//! binary: two teams in one temporary home, with `fake-herdr` first on
//! `PATH` so no real pane is ever typed into.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary"
)]
#![expect(
    clippy::unwrap_used,
    reason = "a failure in this integration test fails the test"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test indexes the JSON it checks; a failure there fails the test"
)]

use std::process::{Command, Output, Stdio};

use serde_json::Value;
use teamwire_core::{
    HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode, Role, SessionId, SessionKind,
    TeamName, Timestamp,
};
use teamwire_fakes::{FakeHerdrEnv, TestHome};
use teamwire_store::{MessageFilter, NewMember, NewTeam, Store};

fn at() -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000).unwrap()
}

fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

/// A team with its lead in session `lead-<name>` and the given members,
/// none of them with a live session.
fn add_team(home: &TestHome, name: &str, members: &[(&str, MemberKind)]) {
    let team = TeamName::parse(name).unwrap();
    let mut store = Store::open(home.path(), &team).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(),
                &NewTeam {
                    name: team.clone(),
                    title: format!("Team {name}"),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((
                        SessionId::parse(&format!("lead-{name}")).unwrap(),
                        SessionKind::Interactive,
                    )),
                },
            )?;
            for (role, kind) in members {
                let r = Role::parse(role).unwrap();
                tx.insert_member(
                    at(),
                    &NewMember {
                        role: r.clone(),
                        title: format!("the {role}"),
                        kind: *kind,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/tmp".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_session(&r, &SessionId::parse(&format!("s-{name}-{role}")).unwrap())?;
                tx.set_member_token_hash(&r, &teamwire_msg::token_hash(&token_of(role)))?;
                tx.set_member_state(at(), &r, MemberState::Started, &serde_json::json!({}))?;
            }
            Ok(())
        })
        .unwrap();
}

struct Rig {
    herdr: FakeHerdrEnv,
    home: TestHome,
}

impl Rig {
    fn new() -> Self {
        let rig = Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
        };
        add_team(&rig.home, "alpha", &[("api", MemberKind::Implementer)]);
        add_team(
            &rig.home,
            "beta",
            &[
                ("db", MemberKind::Implementer),
                ("rev", MemberKind::Reviewer),
            ],
        );
        rig
    }

    /// `teamwire <args>` as member `role` of team alpha, or with no
    /// identity when `role` is `None`.
    fn run(&self, args: &[&str], role: Option<&str>) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
        cmd.args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(role) = role {
            cmd.env("TEAMWIRE_TEAM", "alpha")
                .env("TEAMWIRE_ROLE", role)
                .env("TEAMWIRE_TOKEN", token_of(role));
        }
        cmd.output().unwrap()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn teams_lists_every_team_with_how_to_address_it() {
    let rig = Rig::new();
    let out = rig.run(&["teams"], None);
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{stdout} / {}",
        text(&out.stderr)
    );
    assert!(
        stdout.contains("alpha (open) Team alpha: address as lead@alpha or <role>@alpha"),
        "{stdout}"
    );
    assert!(stdout.contains("beta (open) Team beta"), "{stdout}");
    assert!(stdout.contains("  db "), "{stdout}");

    let out = rig.run(&["teams", "--json"], None);
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = json["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
    assert_eq!(
        json["teams"][1]["members"][1]["accepts_from_members"],
        serde_json::json!([])
    );
}

#[test]
fn an_afk_team_is_marked_in_text_and_carries_its_budget_in_json() {
    let rig = Rig::new();
    let mut beta = Store::open(rig.home.path(), &TeamName::parse("beta").unwrap()).unwrap();
    beta.write(|tx| {
        tx.connection()
            .execute(
                "UPDATE team SET afk_budget_ends_at = '2999-01-01T00:00:00.000Z'",
                [],
            )
            .map(|_| ())
            .map_err(Into::into)
    })
    .unwrap();
    drop(beta);

    let stdout = text(&rig.run(&["teams"], None).stdout);
    assert!(stdout.contains("alpha (open) Team alpha"), "{stdout}");
    assert!(stdout.contains("beta (open, afk) Team beta"), "{stdout}");

    let out = rig.run(&["teams", "--json"], None);
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["teams"][0]["afk_budget_ends_at"], Value::Null);
    assert!(json["teams"][0].get("afk_budget_ends_at").is_some());
    assert_eq!(
        json["teams"][1]["afk_budget_ends_at"],
        "2999-01-01T00:00:00.000Z"
    );
    assert_eq!(json["teams"][1]["name"], "beta", "the roster keys stay");
}

#[test]
fn a_member_sends_to_another_team_and_the_copy_is_stored_there() {
    let rig = Rig::new();
    let out = rig.run(
        &[
            "send",
            "--to",
            "db@beta",
            "--type",
            "finding",
            "--text",
            "the port is 8443",
        ],
        Some("api"),
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    // db has no live session, so the line is not typed: exit 4, stored anyway.
    assert_eq!(out.status.code(), Some(4), "{stdout} / {stderr}");
    assert!(stdout.contains("db@beta: undelivered"), "{stdout}");
    let beta = Store::open_read_only(rig.home.path(), &TeamName::parse("beta").unwrap()).unwrap();
    let stored = beta
        .read(|tx| tx.messages(&MessageFilter::default())?.all_or_corrupt())
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].sender.as_str(), "api@alpha");
    assert_eq!(stored[0].body, "the port is 8443");
}

#[test]
fn a_send_to_a_quiet_member_or_an_unknown_team_is_refused() {
    let rig = Rig::new();
    for (to, code) in [
        ("rev@beta", "recipient_not_allowed"),
        ("db@gamma", "unknown_team"),
    ] {
        let out = rig.run(
            &["send", "--to", to, "--type", "finding", "--text", "x"],
            Some("api"),
        );
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{to}: {stderr}");
        assert!(
            stderr.contains(&format!("refused.{code}")),
            "{to}: {stderr}"
        );
    }
}
