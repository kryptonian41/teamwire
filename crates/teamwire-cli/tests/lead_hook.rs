//! `teamwire hook claude-lead-stop`, run as the real binary Claude Code
//! starts in the lead's session. It exits 0 and writes nothing whatever
//! happens.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary as a harness would"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index JSON; a failure there fails the test"
)]

#[path = "../../teamwire-verify/tests/support/fixture.rs"]
pub mod fixture;

use std::io::Write;
use std::process::{Command, Stdio};

use fixture::{Fixture, MemberSetup};
use teamwire_core::{Clock, MemberKind, Mode, Pid, SessionId, SessionKind};

struct Run {
    code: i32,
    out: String,
}

/// A team led from session `lead-1` with member `api` started and not done.
fn team() -> Fixture {
    let mut fx = Fixture::new();
    fx.add_member(&MemberSetup {
        role: "api",
        kind: MemberKind::Implementer,
        mode: Mode::Pane,
        check: None,
        owns: &[],
        base_sha: None,
        cwd: std::env::temp_dir(),
    });
    let at = teamwire_core::Clock::now(&fx.clock);
    fx.store
        .write(|tx| {
            tx.adopt_lead(
                at,
                &SessionId::parse("lead-1").unwrap(),
                SessionKind::Interactive,
            )
            .map(|_| ())
        })
        .unwrap();
    fx
}

fn hook(fx: &Fixture, env: &[(&str, &str)], stdin: &str) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(fx.home.path())
        .args(["hook", "claude-lead-stop"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
    }
}

fn stop(session: &str, extra: &str) -> String {
    format!(
        r#"{{"session_id":"{session}","hook_event_name":"Stop","stop_hook_active":false{extra}}}"#
    )
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::last_event_seq` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn events(fx: &Fixture) -> u64 {
    fx.store.read(|tx| tx.last_event_seq()).unwrap()
}

#[test]
fn holds_the_stop_once_and_names_the_team() {
    let fx = team();
    let before = events(&fx);
    let run = hook(&fx, &[], &stop("lead-1", ""));
    assert_eq!(run.code, 0);
    let reply: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(reply["hookSpecificOutput"]["hookEventName"], "Stop");
    assert_eq!(
        reply["hookSpecificOutput"]["additionalContext"],
        "[teamwire:alpha] no watch is running; start `teamwire watch alpha` with Monitor before you stop."
    );
    assert_eq!(events(&fx), before, "the hook writes nothing");
}

#[test]
fn a_continuation_is_not_held_again() {
    let fx = team();
    let run = hook(&fx, &[], &stop("lead-1", "").replace("false", "true"));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn a_subagent_stop_is_not_held() {
    let fx = team();
    let run = hook(&fx, &[], &stop("lead-1", r#","agent_id":"sub-1""#));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn a_member_session_returns_at_once() {
    let fx = team();
    let run = hook(&fx, &[("TEAMWIRE_TEAM", "alpha")], &stop("lead-1", ""));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn another_session_is_not_held() {
    let fx = team();
    let run = hook(&fx, &[], &stop("other-session", ""));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn a_running_watch_is_not_reported_missing() {
    let mut fx = team();
    // This test process holds the lease and is alive. The binary reads the
    // real clock, so the heartbeat is stamped with the real time.
    let at = teamwire_platform::SystemClock.now();
    fx.store
        .write(|tx| {
            tx.take_watch_lease(at, Pid::new(std::process::id()), "host", |_| false)
                .map(|_| ())
        })
        .unwrap();
    let run = hook(&fx, &[], &stop("lead-1", ""));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn bad_input_and_a_missing_home_exit_zero_with_no_output() {
    let fx = team();
    for input in ["", "not json {{", "[1,2,3]", "{}", "\u{0}\u{1}"] {
        let run = hook(&fx, &[], input);
        assert_eq!((run.code, run.out.as_str()), (0, ""), "{input:?}");
    }
    let empty = Fixture::new();
    let run = hook(&empty, &[], &stop("lead-1", ""));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}
