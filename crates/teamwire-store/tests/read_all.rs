//! The read methods `teamwire export` uses to cover every table: all
//! verify records, all completions and all counters, whatever the members
//! table holds.
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use teamwire_core::{
    CompletionStatus, FinalSource, HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, TeamName,
    Timestamp,
};
use teamwire_fakes::TestHome;
use teamwire_store::{NewCompletion, NewMember, NewTeam, NewVerifyRecord, Store, database_path};

fn at(n: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + n).unwrap()
}

fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

fn setup(home: &TestHome, members: &[&str]) -> Store {
    let mut store = Store::open(home.path(), &team()).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: team(),
                    title: "t".to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            for name in members {
                tx.insert_member(
                    at(1),
                    &NewMember {
                        role: role(name),
                        title: "t".to_owned(),
                        kind: MemberKind::Reviewer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/w".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec!["src/".to_owned()],
                        deadline_s: 60,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    store
}

#[test]
fn every_role_s_verify_records_completions_and_counters_are_listed() {
    let home = TestHome::new().unwrap();
    let mut store = setup(&home, &["api", "web"]);
    store
        .write(|tx| {
            for who in ["api", "web"] {
                tx.insert_verify_record(
                    at(2),
                    &NewVerifyRecord {
                        role: role(who),
                        head_sha: "abc".to_owned(),
                        verdict: "pass".to_owned(),
                        exit_code: Some(0),
                        seconds: Some(1),
                        flags: vec![],
                        log_sha256: Some([7; 32]),
                    },
                )?;
                tx.insert_completion(
                    at(3),
                    &NewCompletion {
                        role: role(who),
                        generation: 1,
                        status: CompletionStatus::Done,
                        summary: "s".to_owned(),
                        final_message: None,
                        final_source: FinalSource::None,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let verifies = store.read(|tx| tx.all_verify_records()).unwrap();
    let roles: Vec<&str> = verifies
        .items
        .iter()
        .map(|v| v.record.role.as_str())
        .collect();
    assert_eq!(roles, ["api", "web"]);
    let completions = store.read(|tx| tx.completions()).unwrap();
    assert_eq!(completions.items.len(), 2);
    let counters = store.read(|tx| tx.counters()).unwrap();
    let names: Vec<&str> = counters.items.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"completion:api") && names.contains(&"completion:web"),
        "{names:?}"
    );
    drop(store);

    // A damaged member row hides nothing from `completions`.
    let conn = rusqlite::Connection::open(database_path(home.path(), &team())).unwrap();
    conn.execute(
        "UPDATE members SET gen_started_at = 'yesterday-ish' WHERE role = 'api'",
        [],
    )
    .unwrap();
    drop(conn);
    let store = Store::open_read_only(home.path(), &team()).unwrap();
    assert_eq!(store.read(|tx| tx.completions()).unwrap().items.len(), 2);
}
