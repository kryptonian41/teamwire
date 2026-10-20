//! `teamwire verify`, run as the real binary against a team in a temporary
//! `$TEAMWIRE_HOME` and a real git repository.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and stands in for a person who edits files on disk"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-verify/tests/support/fixture.rs"]
pub mod fixture;

use std::process::Output;

use fixture::{Fixture, Repo, implementer, start_secs};
use teamwire_core::{Actor, MessageType, Recipient, Role};
use teamwire_store::NewMessage;

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(fx: &Fixture, args: &[&str]) -> Run {
    teamwire_with(fx, args, &[])
}

fn teamwire_with(fx: &Fixture, args: &[&str], extra: &[(&str, &str)]) -> Run {
    let Output {
        status,
        stdout,
        stderr,
    } = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(fx.home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .env_remove("TEAMWIRE_TEAM")
        .env_remove("TEAMWIRE_ROLE")
        .env_remove("TEAMWIRE_TOKEN")
        .env("TEAMWIRE_HOME", fx.home.path())
        .envs(extra.iter().copied())
        .output()
        .unwrap();
    Run {
        code: status.code().unwrap(),
        out: String::from_utf8(stdout).unwrap(),
        err: String::from_utf8(stderr).unwrap(),
    }
}

/// A team of one implementer whose branch has one commit, a report for it, and `check`.
fn team(check: &str, own_files: &[(&str, &str)]) -> (Fixture, Repo) {
    let repo = Repo::init(
        &[("tests/a.rs", "#[test]\nfn a() {}\n")],
        start_secs() - 100,
    );
    repo.git(&["checkout", "--quiet", "-b", "work"]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some(check), &["src/", "tests/"]));
    let sha = repo.commit(own_files, "own work", start_secs() + 5);
    fx.write_report(
        "api",
        &format!("- {} did it\n\nDecisions I made: none\n", &sha[..7]),
    );
    (fx, repo)
}

#[test]
fn exit_zero_and_clean_only_when_every_member_passes() {
    let (fx, _repo) = team("exit 0", &[("src/lib.rs", "fn a() {}\n")]);
    let run = teamwire(&fx, &["verify", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("api          PASS"), "{}", run.out);
    assert!(run.out.contains("exit 0 in"), "{}", run.out);
    assert!(run.out.trim_end().ends_with("clean"), "{}", run.out);
}

#[test]
fn a_failing_check_exits_two_and_names_the_log() {
    let (fx, _repo) = team("echo boom >&2; exit 3", &[("src/lib.rs", "")]);
    let run = teamwire(&fx, &["verify", "alpha", "--role", "api"]);
    assert_eq!(run.code, 2, "{}", run.out);
    assert!(run.out.contains("FAIL"), "{}", run.out);
    assert!(run.out.contains("exit 3"), "{}", run.out);
    assert!(run.out.contains("api.log"), "{}", run.out);
    assert!(run.out.contains("NOT clean"), "{}", run.out);
}

#[test]
fn a_flagged_pass_exits_two_until_the_flag_is_waived_for_that_head() {
    let (fx, repo) = team(
        "exit 0",
        &[("src/lib.rs", "#[allow(dead_code)]\nfn a() {}\n")],
    );
    let run = teamwire(&fx, &["verify", "alpha"]);
    assert_eq!(run.code, 2, "{}", run.out);
    assert!(run.out.contains("FLAGGED"), "{}", run.out);
    assert!(run.out.contains("! [weakened:src/lib.rs:1]"), "{}", run.out);

    let refused = teamwire(
        &fx,
        &[
            "verify",
            "alpha",
            "--role",
            "api",
            "--waive",
            "weakened:src/lib.rs:9",
            "--reason",
            "x",
        ],
    );
    assert_eq!(refused.code, 1);
    assert!(refused.err.contains("no flag"), "{}", refused.err);

    let waived = teamwire(
        &fx,
        &[
            "verify",
            "alpha",
            "--role",
            "api",
            "--waive",
            "weakened:src/lib.rs:1",
            "--reason",
            "dead code is on purpose",
        ],
    );
    assert_eq!(waived.code, 0, "{}{}", waived.out, waived.err);
    let run = teamwire(&fx, &["verify", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.out);
    assert!(run.out.contains("(waived)"), "{}", run.out);
    assert!(run.out.contains("PASS"), "{}", run.out);

    // The waiver does not follow a new commit.
    let sha = repo.commit(&[("src/more.rs", "fn m() {}\n")], "more", start_secs() + 9);
    let text = std::fs::read_to_string(fx.team_dir().join("reports/api.md")).unwrap();
    fx.write_report("api", &format!("{text}- {} more\n", &sha[..7]));
    let run = teamwire(&fx, &["verify", "alpha"]);
    assert_eq!(run.code, 2, "{}", run.out);
    assert!(run.out.contains("FLAGGED"), "{}", run.out);
}

#[test]
fn an_unknown_role_and_an_unknown_team_are_refused() {
    let (fx, _repo) = team("exit 0", &[("src/lib.rs", "")]);
    let run = teamwire(&fx, &["verify", "alpha", "--role", "nobody"]);
    assert_eq!(run.code, 1);
    assert!(run.err.contains("no member nobody"), "{}", run.err);
    let run = teamwire(&fx, &["verify", "beta"]);
    assert_eq!(run.code, 1);
    assert!(run.err.contains("no database for team beta"), "{}", run.err);
    let run = teamwire(&fx, &["verify", "Bad-Team"]);
    assert_eq!(run.code, 3, "{}", run.err);
}

#[test]
fn repair_files_writes_a_missing_copy_and_the_run_is_clean() {
    let (mut fx, _repo) = team("exit 0", &[("src/lib.rs", "")]);
    let at = teamwire_core::Clock::now(&fx.clock);
    fx.store
        .write(|tx| {
            tx.insert_message(
                at,
                &NewMessage {
                    sender: Actor::Member(Role::parse("api").unwrap()),
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: "hello lead".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Lead],
                },
            )
            .map(|_| ())
        })
        .unwrap();
    let copy = fx.team_dir().join("comms").join("m001.md");

    let run = teamwire(&fx, &["verify", "alpha"]);
    assert_eq!(run.code, 2, "{}", run.out);
    assert!(run.out.contains("comms/m001.md: missing"), "{}", run.out);
    assert!(!copy.exists());

    let run = teamwire(&fx, &["verify", "alpha", "--repair-files"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.contains("repaired teams/alpha/comms/m001.md"),
        "{}",
        run.out
    );
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "hello lead");
}

#[test]
fn deep_prints_the_integrity_result() {
    let (fx, _repo) = team("exit 0", &[("src/lib.rs", "")]);
    let run = teamwire(&fx, &["verify", "alpha", "--deep"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("integrity_check: ok"), "{}", run.out);
}

#[test]
fn a_member_session_cannot_waive_a_flag_but_can_run_verify() {
    let (fx, _repo) = team(
        "exit 0",
        &[("src/lib.rs", "#[allow(dead_code)]\nfn a() {}\n")],
    );
    let member = [
        ("TEAMWIRE_TEAM", "alpha"),
        ("TEAMWIRE_ROLE", "api"),
        ("TEAMWIRE_TOKEN", "not-a-real-token"),
    ];
    let waive = teamwire_with(
        &fx,
        &[
            "verify",
            "alpha",
            "--role",
            "api",
            "--waive",
            "weakened:src/lib.rs:1",
            "--reason",
            "mine",
        ],
        &member,
    );
    assert_eq!(waive.code, 1, "{}{}", waive.out, waive.err);
    assert!(waive.err.contains("refused.identity"), "{}", waive.err);
    // The refused waiver left the flag in place, and plain verify still runs.
    let run = teamwire_with(&fx, &["verify", "alpha"], &member);
    assert_eq!(run.code, 2, "{}{}", run.out, run.err);
    assert!(run.out.contains("FLAGGED"), "{}", run.out);
}

#[test]
fn a_store_failure_leaves_a_row_named_for_the_command() {
    let (mut fx, _repo) = team("exit 0", &[("src/lib.rs", "fn a() {}\n")]);
    fx.store
        .write(|tx| {
            // Valid JSON, so the CHECK passes, but not a list of paths.
            tx.connection()
                .execute("UPDATE members SET owns = '{}'", [])?;
            Ok(())
        })
        .unwrap();
    let run = teamwire(&fx, &["verify", "alpha"]);
    assert_eq!(run.code, 6, "{}{}", run.out, run.err);
    let errors: Vec<serde_json::Value> = fx
        .store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == teamwire_core::EventType::StoreError)
        .map(|e| e.data)
        .collect();
    assert_eq!(
        errors,
        [serde_json::json!({ "op": "verify", "code": "corrupt" })]
    );
}
