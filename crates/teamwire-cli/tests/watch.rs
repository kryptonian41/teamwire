//! `teamwire watch`, run as the real binary against a team in a temporary
//! `$TEAMWIRE_HOME`. The team's members name adapters this binary does not
//! have, so the watch makes no call to any real herdr or Claude.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::watch_lease` as a path does not infer the transaction's lifetime for `Store::read`"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;

use fixture::Bed;
use teamwire_core::{Clock, CompletionStatus, Pid};

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(bed: &Bed, args: &[&str]) -> Run {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .env_remove("TEAMWIRE_TOKEN")
        .env_remove("TEAMWIRE_ROLE")
        .env("TEAMWIRE_HOME", bed.home.path())
        .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
        err: String::from_utf8(out.stderr).unwrap(),
    }
}

#[test]
fn once_prints_one_line_and_exits_zero() {
    // The binary reads the real clock and the fake clock stands in 2026, so
    // give the members a deadline that has not passed.
    let mut bed = Bed::new();
    bed.add_with_deadline("api", 100_000_000);
    bed.add_with_deadline("web", 100_000_000);
    let run = teamwire(&bed, &["watch", "alpha", "--once"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(run.out, "[teamwire:alpha] WATCHING 2 members\n");
    // Nothing is left behind: the lease is released.
    assert!(bed.store.read(|tx| tx.watch_lease()).unwrap().is_none());
}

#[test]
fn a_finished_team_ends_the_watch_without_once() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.add("web");
    bed.delivered("api", CompletionStatus::Done);
    bed.delivered("web", CompletionStatus::Failed);
    let run = teamwire(&bed, &["watch", "alpha", "--interval", "1"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(
        run.out,
        "[teamwire:alpha] WATCHING 2 members | [teamwire:alpha] TEAM-COMPLETE 1/2 done; not done: web failed\n"
    );
}

#[test]
fn a_second_watch_is_refused_and_names_the_first() {
    let mut bed = Bed::new();
    bed.add("api");
    // This test process holds the lease, and it is alive.
    let me = Pid::new(std::process::id());
    let at = bed.clock.now();
    bed.store
        .write(|tx| {
            tx.take_watch_lease(at, me, "otherhost", |_| false)
                .map(|_| ())
        })
        .unwrap();
    let run = teamwire(&bed, &["watch", "alpha", "--once"]);
    assert_eq!(run.code, 1, "{}", run.out);
    assert!(run.out.is_empty());
    assert!(run.err.contains(&me.to_string()), "{}", run.err);
    assert!(run.err.contains("otherhost"), "{}", run.err);
}

#[test]
fn a_dead_watchs_lease_is_taken_over() {
    let mut bed = Bed::new();
    bed.add("api");
    // A pid no live process has: far above any real pid limit.
    let gone = Pid::new(2_000_000_000);
    let at = bed.clock.now();
    bed.store
        .write(|tx| {
            tx.take_watch_lease(at, gone, "otherhost", |_| false)
                .map(|_| ())
        })
        .unwrap();
    let run = teamwire(&bed, &["watch", "alpha", "--once"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
}

#[test]
fn a_closed_team_ends_the_watch_silently() {
    let mut bed = Bed::new();
    bed.add("api");
    let at = bed.clock.now();
    bed.store
        .write(|tx| {
            tx.begin_close(at, Pid::new(1))?;
            tx.finish_close(at, &teamwire_store::CloseEnd::Completed)
        })
        .unwrap();
    let run = teamwire(&bed, &["watch", "alpha", "--interval", "1"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.is_empty(), "{}", run.out);
}

#[test]
fn an_unknown_team_is_refused() {
    let bed = Bed::new();
    let run = teamwire(&bed, &["watch", "nosuch", "--once"]);
    assert_eq!(run.code, 1);
    assert!(run.err.contains("no database"), "{}", run.err);
}

#[test]
fn a_bad_team_name_or_interval_is_a_usage_error() {
    let bed = Bed::new();
    assert_eq!(teamwire(&bed, &["watch", "No-Good", "--once"]).code, 3);
    assert_eq!(
        teamwire(&bed, &["watch", "alpha", "--interval", "0", "--once"]).code,
        3
    );
}

#[test]
fn a_store_from_a_newer_binary_exits_six() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.store
        .write(|tx| {
            tx.connection()
                .execute_batch("PRAGMA user_version = 2")
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let run = teamwire(&bed, &["watch", "alpha", "--once"]);
    assert_eq!(run.code, 6, "{}{}", run.out, run.err);
    assert!(run.err.contains("schema"), "{}", run.err);
}

#[test]
fn an_export_endpoint_does_not_change_the_output() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.delivered("api", CompletionStatus::Done);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(["watch", "alpha", "--interval", "1"])
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:1")
        .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
        .env_remove("HERDR_ENV")
        .env_remove("TEAMWIRE_TOKEN")
        .env_remove("TEAMWIRE_ROLE")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "[teamwire:alpha] WATCHING 1 members | [teamwire:alpha] TEAM-COMPLETE 1/1 done\n"
    );
}
