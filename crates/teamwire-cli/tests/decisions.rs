//! `teamwire decisions`, run as the real binary against teams and archives in
//! a temporary `$TEAMWIRE_HOME` with made-up reports.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use teamwire_core::{HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, TeamName, Timestamp};
use teamwire_fakes::TestHome;
use teamwire_store::{NewMember, NewTeam, Store};

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(home: &TestHome, args: &[&str]) -> Run {
    let Output {
        status,
        stdout,
        stderr,
    } = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(args)
        .env("TEAMWIRE_HOME", home.path())
        .output()
        .unwrap();
    Run {
        code: status.code().unwrap(),
        out: String::from_utf8(stdout).unwrap(),
        err: String::from_utf8(stderr).unwrap(),
    }
}

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

/// A team created at `secs` with one implementer per role.
fn team(home: &Path, name: &str, secs: i64, roles: &[&str]) -> Store {
    let team = TeamName::parse(name).unwrap();
    let mut store = Store::open(home, &team).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(secs),
                &NewTeam {
                    name: team.clone(),
                    title: format!("Team {name}"),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            for role in roles {
                tx.insert_member(
                    at(secs),
                    &NewMember {
                        role: Role::parse(role).unwrap(),
                        title: format!("the {role}"),
                        kind: MemberKind::Implementer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: home.join("work"),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    store
}

fn report(dir: &Path, role: &str, text: &str) {
    let reports = dir.join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(reports.join(format!("{role}.md")), text).unwrap();
}

/// Copies `store` into `archive/<folder>` the way a close does, with only
/// `team.db` and a checksum file that is `age_s` seconds old.
fn archive(home: &Path, store: &Store, folder: &str, age_s: u64) -> PathBuf {
    let dir = home.join("archive").join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    store.backup_to(&dir.join("team.db")).unwrap();
    let sums = std::fs::File::create(dir.join("SHA256SUMS")).unwrap();
    sums.set_modified(SystemTime::now() - Duration::from_secs(age_s))
        .unwrap();
    dir
}

#[test]
fn a_running_team_prints_its_blocks_and_marks_a_missing_report() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web", "api"]);
    report(
        &home.path().join("teams/alpha"),
        "web",
        "## Decisions I made\n- used a table\n",
    );
    let run = teamwire(&home, &["decisions", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("== alpha (running)"), "{}", run.out);
    assert!(run.out.contains("- used a table"), "{}", run.out);
    assert!(run.out.contains("(no report)"), "{}", run.out);
}

#[test]
fn role_and_json_narrow_and_shape_the_output() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web", "api"]);
    report(
        &home.path().join("teams/alpha"),
        "api",
        "Decisions I made: none\n",
    );
    let run = teamwire(&home, &["decisions", "alpha", "--role", "api", "--json"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let view: Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(view["errors"], serde_json::json!([]));
    let members = view["teams"][0]["members"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["role"], "api");
    assert_eq!(members[0]["blocks"][0]["text"], "none");
    assert_eq!(view["teams"][0]["live"], true);
}

#[test]
fn only_the_newest_archive_of_a_closed_team_is_read() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    let store = team(other.path(), "beta", 2, &["web"]);
    let old = archive(home.path(), &store, "beta-2026-09-01", 3600);
    let new = archive(home.path(), &store, "beta-2026-09-02", 60);
    report(&old, "web", "## Decisions I made\n- from the old archive\n");
    report(&new, "web", "## Decisions I made\n- from the new archive\n");
    let run = teamwire(&home, &["decisions", "beta"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("(archived)"), "{}", run.out);
    assert!(run.out.contains("from the new archive"), "{}", run.out);
    assert!(!run.out.contains("old archive"), "{}", run.out);
}

#[test]
fn grep_searches_every_team_and_archive() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web"]);
    report(
        &home.path().join("teams/alpha"),
        "web",
        "## Decisions I made\n- kept the Stash out\n",
    );
    let gamma = team(other.path(), "gamma", 3, &["db"]);
    let dir = archive(home.path(), &gamma, "gamma-2026-09-30", 60);
    report(&dir, "db", "Decisions I made:\n- no stash here\n- more\n");
    let run = teamwire(&home, &["decisions", "--grep", "STASH"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("== alpha (running)"), "{}", run.out);
    assert!(run.out.contains("== gamma (archived)"), "{}", run.out);
    assert!(run.out.contains("- more"), "{}", run.out);
}

#[test]
fn no_team_and_no_grep_is_a_usage_error() {
    let home = TestHome::new().unwrap();
    let run = teamwire(&home, &["decisions"]);
    assert_eq!(run.code, 3);
    assert!(
        run.err.contains("name a team or give --ref or --grep"),
        "{}",
        run.err
    );
}

#[test]
fn an_unknown_team_is_refused() {
    let home = TestHome::new().unwrap();
    let run = teamwire(&home, &["decisions", "nope"]);
    assert_eq!(run.code, 1);
    assert!(run.err.contains("no such team or archive"), "{}", run.err);
}

#[test]
fn only_errors_exit_1_and_something_printed_exits_0() {
    let home = TestHome::new().unwrap();
    let broken = home.path().join("teams/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("team.db"), b"not a database").unwrap();
    let run = teamwire(&home, &["decisions", "--grep", "x"]);
    assert_eq!(run.code, 1);
    assert!(run.err.contains("broken"), "{}", run.err);
    team(home.path(), "alpha", 1, &["web"]);
    report(
        &home.path().join("teams/alpha"),
        "web",
        "## Decisions I made\n- x\n",
    );
    let run = teamwire(&home, &["decisions", "--grep", "x"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("== alpha"), "{}", run.out);
}

#[test]
fn ref_selects_teams_and_counts_as_a_filter_and_the_json_has_refs() {
    let home = TestHome::new().unwrap();
    for (name, secs, refs) in [
        ("alpha", 1, vec![("linear", "GKA-580")]),
        ("beta", 2, vec![("jira", "GKA-580")]),
        ("gamma", 3, vec![]),
    ] {
        let mut store = team(home.path(), name, secs, &["web"]);
        let refs: Vec<teamwire_core::TicketRef> = refs
            .into_iter()
            .map(|(p, id)| teamwire_core::TicketRef {
                provider: p.to_owned(),
                id: id.to_owned(),
            })
            .collect();
        store.write(|tx| tx.set_refs(&refs)).unwrap();
        report(
            &home.path().join("teams").join(name),
            "web",
            "## Decisions I made\n- x\n",
        );
    }
    let run = teamwire(&home, &["decisions", "--ref", "linear:GKA-580", "--json"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let view: Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(view["teams"].as_array().unwrap().len(), 1);
    assert_eq!(
        view["teams"][0]["refs"],
        serde_json::json!([{"provider": "linear", "id": "GKA-580"}])
    );
    let run = teamwire(&home, &["decisions", "--ref", "gka-580"]);
    assert!(
        run.out.contains("== alpha") && run.out.contains("== beta"),
        "{}",
        run.out
    );
    assert!(run.out.contains("[linear:GKA-580]"), "{}", run.out);
    assert!(!run.out.contains("== gamma"), "{}", run.out);
}
