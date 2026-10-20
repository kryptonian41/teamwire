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
    hook_at(fx.home.path(), env, stdin)
}

fn hook_at(home: &std::path::Path, env: &[(&str, &str)], stdin: &str) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(home)
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
    for env in [
        &[("TEAMWIRE_TEAM", "alpha")][..],
        &[("TEAMWIRE_TOKEN", "t")],
    ] {
        let run = hook(&fx, env, &stop("lead-1", ""));
        assert_eq!((run.code, run.out.as_str()), (0, ""), "{env:?}");
    }
}

#[test]
fn an_empty_team_variable_is_still_the_lead() {
    let fx = team();
    let run = hook(&fx, &[("TEAMWIRE_TEAM", "")], &stop("lead-1", ""));
    assert_eq!(run.code, 0);
    assert!(
        run.out.contains("[teamwire:alpha] no watch is running"),
        "{}",
        run.out
    );
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

/// Marks the fixture's team as an AFK run whose budget ends at `ends`.
fn make_afk(fx: &mut Fixture, ends: &str) {
    fx.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE team SET afk_budget_ends_at = ?1", [ends])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
}

fn context(run: &Run) -> String {
    let reply: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    reply["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn an_afk_team_with_nothing_to_wake_the_lead_is_held_with_the_afk_note() {
    let mut fx = team();
    make_afk(&mut fx, "2999-01-01T00:00:00.000Z");
    // No member is working, so only the AFK note applies.
    fx.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET state = 'creating'", [])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    let before = events(&fx);
    let run = hook(&fx, &[], &stop("lead-1", ""));
    assert_eq!(run.code, 0);
    let dir = fx.team_dir().display().to_string();
    assert_eq!(
        context(&run),
        format!(
            "[teamwire:alpha] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to {dir}/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close alpha` once {dir}/afk/REPORT.md exists. If a command you started will wake you, stop again."
        )
    );
    assert_eq!(events(&fx), before, "the hook writes nothing");
}

#[test]
fn both_notes_come_back_in_one_reply() {
    let mut fx = team();
    make_afk(&mut fx, "2020-01-01T00:00:00.000Z");
    let run = hook(&fx, &[], &stop("lead-1", ""));
    let text = context(&run);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].starts_with("[teamwire:alpha] no watch is running;"));
    assert!(
        lines[1].starts_with("[teamwire:alpha] budget spent at 2020-01-01T00:00:00.000Z: "),
        "{text}"
    );
}

#[test]
fn an_afk_continuation_is_not_held_again() {
    let mut fx = team();
    make_afk(&mut fx, "2999-01-01T00:00:00.000Z");
    let run = hook(&fx, &[], &stop("lead-1", "").replace("false", "true"));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn an_afk_team_with_a_working_member_and_a_live_watch_is_not_held() {
    let mut fx = team();
    make_afk(&mut fx, "2999-01-01T00:00:00.000Z");
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

/// Four teams' databases are each held under an exclusive lock by another
/// process, so each read waits for the database engine's own 5 s busy
/// timeout. Read one after the other that is at least 20 s; the hook must
/// give up at its own 5 s bound and let the stop through with no output.
///
/// What this proves: the whole hook, the part after its bound included,
/// ends near the bound however many stores are locked. What it cannot
/// prove: that the part after the bound reads no store. A hook whose later
/// part still read stores passes too, because once the bounded part gives
/// up the hook lets the stop through and nothing runs after it. Catching
/// that needs a lock that starts between the bounded reads and the later
/// ones, which a test cannot time reliably. The signature of
/// `lead_reads_watch_lines`, which takes no path, guards it instead.
#[test]
fn locked_stores_let_the_stop_through_at_the_bound() {
    let fx = team();
    // Close the test's own connection and leave WAL mode: a WAL reader is not
    // blocked by a lock, a rollback-journal reader is.
    let home = fx.home.path().to_path_buf();
    drop(fx.store);
    let db = |name: &str| {
        teamwire_store::database_path(&home, &teamwire_core::TeamName::parse(name).unwrap())
    };
    let teams = ["alpha", "beta", "gamma", "delta"];
    for name in &teams[1..] {
        std::fs::create_dir_all(db(name).parent().unwrap()).unwrap();
        std::fs::copy(db("alpha"), db(name)).unwrap();
    }
    let mut holders = Vec::new();
    for name in teams {
        let mut holder = Command::new("sqlite3")
            .arg(db(name))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut held = holder.stdin.take().unwrap();
        held.write_all(b"PRAGMA journal_mode=DELETE;\nBEGIN EXCLUSIVE;\nSELECT 1;\n")
            .unwrap();
        held.flush().unwrap();
        holders.push((holder, held));
    }
    // Time for the holders to take their locks before the hook starts.
    std::thread::sleep(std::time::Duration::from_millis(500));

    let start = std::time::Instant::now();
    let run = hook_at(&home, &[], &stop("lead-1", ""));
    let waited = start.elapsed();
    for (mut holder, held) in holders {
        drop(held);
        holder.kill().ok();
        holder.wait().ok();
    }

    assert_eq!((run.code, run.out.as_str()), (0, ""));
    // Not much below 5 s: a hook that failed fast on the lock would also print
    // nothing, and only the wait shows it used its bound.
    assert!(waited >= std::time::Duration::from_secs(4), "{waited:?}");
    // Well under the 20 s that four unbounded reads take, yet loose enough
    // for process start and exit on a loaded machine.
    assert!(waited < std::time::Duration::from_secs(15), "{waited:?}");
}
