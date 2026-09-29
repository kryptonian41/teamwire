//! `teamwire done`, run as the real binary as a member would run it.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary as a harness would"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-verify/tests/support/fixture.rs"]
pub mod fixture;

use std::process::{Command, Stdio};

use fixture::{Fixture, MemberSetup};
use teamwire_core::{MemberKind, Mode, Role};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn team() -> Fixture {
    let mut fx = Fixture::new();
    fx.add_member(&MemberSetup {
        role: "api",
        kind: MemberKind::Investigator,
        mode: Mode::Pane,
        check: None,
        owns: &[],
        base_sha: None,
        cwd: std::env::temp_dir(),
    });
    let api = Role::parse("api").unwrap();
    fx.store
        .write(|tx| tx.set_member_token_hash(&api, &teamwire_report::hash_token(TOKEN)))
        .unwrap();
    fx
}

fn done(fx: &Fixture, env: &[(&str, &str)], args: &[&str]) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(fx.home.path())
        .arg("done")
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
        err: String::from_utf8(out.stderr).unwrap(),
    }
}

fn as_api() -> [(&'static str, &'static str); 3] {
    [
        ("TEAMWIRE_TEAM", "alpha"),
        ("TEAMWIRE_ROLE", "api"),
        ("TEAMWIRE_TOKEN", TOKEN),
    ]
}

fn stored(fx: &Fixture) -> usize {
    let api = Role::parse("api").unwrap();
    fx.store
        .read(|tx| tx.completions_of(&api))
        .unwrap()
        .items
        .len()
}

#[test]
fn invalid_status_refused() {
    let fx = team();
    let run = done(&fx, &as_api(), &["--status", "finished", "--summary", "x"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("status"), "{}", run.err);
    assert_eq!(stored(&fx), 0, "a refused call stores nothing");
    let refused = fx
        .store
        .read(|tx| tx.events_of_type(teamwire_core::EventType::MemberCompletionRefused))
        .unwrap();
    assert_eq!(refused.items.len(), 1, "the refusal is recorded");
}

#[test]
fn a_member_records_its_completion() {
    let fx = team();
    let run = done(
        &fx,
        &as_api(),
        &["--status", "done", "--summary", "all fixed"],
    );
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.contains("recorded completion 1 (done)"),
        "{}",
        run.out
    );
    assert!(run.out.contains("when this turn ends"), "{}", run.out);
    assert_eq!(stored(&fx), 1);
    assert!(!run.out.contains(TOKEN) && !run.err.contains(TOKEN));
}

#[test]
fn a_wrong_token_or_a_missing_identity_is_refused() {
    let fx = team();
    let mut wrong = as_api();
    wrong[2] = ("TEAMWIRE_TOKEN", "not-the-token");
    let run = done(&fx, &wrong, &["--status", "done", "--summary", "x"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("refused.identity"), "{}", run.err);

    let run = done(&fx, &[], &["--status", "done", "--summary", "x"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("refused.identity"), "{}", run.err);
    assert_eq!(stored(&fx), 0);
}

#[test]
fn a_closed_team_refuses_and_stores_nothing() {
    let mut fx = team();
    let at = teamwire_core::Clock::now(&fx.clock);
    fx.store
        .write(|tx| tx.begin_close(at, teamwire_core::Pid::new(1)).map(|_| ()))
        .unwrap();
    let run = done(&fx, &as_api(), &["--status", "done", "--summary", "x"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("team_closed"), "{}", run.err);
    assert_eq!(stored(&fx), 0);
}
