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

use std::io::BufRead;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fixture::{Bed, events};
use teamwire_core::{Clock, CompletionStatus, Pid, Timestamp};

struct Run {
    code: i32,
    out: String,
    err: String,
}

/// What `TEAM-COMPLETE` adds while the team has an implementer and no closer
/// has reported. Every fixture member is an implementer.
const NO_CLOSER: &str = ". If no wave is due: add the integrator when branches join, then the closer (teamwire preset closer).";

/// How long a test waits for a watch to print something or to exit.
const PATIENCE: Duration = Duration::from_secs(30);

/// Every event the watch printed, line by line, with the prefix of each
/// taken off: `[["WATCHING 1 members"], ["TEAM-COMPLETE 1/1 done"]]`.
fn printed(output: &str) -> Vec<Vec<String>> {
    output.lines().map(events).collect()
}

/// Calls `ready` every 20 ms until it is true or `within` runs out.
fn wait_until(within: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    ready()
}

/// A `teamwire watch` still running, its output read by a thread as the
/// watch prints it. Dropping it kills the process, so a test that fails
/// half way leaves nothing running.
struct Watching {
    child: std::process::Child,
    out: Arc<Mutex<String>>,
}

impl Watching {
    /// Starts `teamwire watch alpha <args>` against the bed's home, with the
    /// same cleared environment `teamwire` uses.
    fn start(bed: &Bed, args: &[&str]) -> Self {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .arg("--home")
            .arg(bed.home.path())
            .args(["watch", "alpha"])
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TEAMWIRE_HOME", bed.home.path())
            .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let out = Arc::new(Mutex::new(String::new()));
        let stdout = child.stdout.take().unwrap();
        let into = Arc::clone(&out);
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                into.lock().unwrap().push_str(&line);
                into.lock().unwrap().push('\n');
            }
        });
        Self { child, out }
    }

    /// Everything the watch has printed so far.
    fn output(&self) -> String {
        self.out.lock().unwrap().clone()
    }

    fn wait_for(&self, needle: &str) {
        let seen = wait_until(PATIENCE, || self.output().contains(needle));
        assert!(seen, "never printed {needle:?}: {}", self.output());
    }

    /// The time of the watch lease's last heartbeat: one poll writes one.
    fn heartbeat(bed: &Bed) -> Option<Timestamp> {
        bed.store
            .read(|tx| tx.watch_lease())
            .ok()
            .flatten()
            .map(|lease| lease.heartbeat_at)
    }

    /// Waits for two polls to finish after the one that wrote `since`. One
    /// poll reads the roster in one transaction and writes its heartbeat in
    /// the next, so the first new heartbeat can come from a poll that read
    /// the store before the test's own write landed. The second poll is sure
    /// to have read it after, as `tests/e2e/tests/add.rs` waits twice for
    /// the same reason.
    fn wait_for_two_polls_after(bed: &Bed, since: Option<Timestamp>) {
        let mut last = since;
        for _ in 0..2 {
            let polled = wait_until(PATIENCE, || {
                Self::heartbeat(bed).is_some_and(|now| Some(now) != last)
            });
            assert!(polled, "the watch stopped polling");
            last = Self::heartbeat(bed);
        }
    }

    fn exit_code(&mut self) -> Option<i32> {
        let mut code = None;
        let done = wait_until(PATIENCE, || match self.child.try_wait() {
            Ok(Some(status)) => {
                code = status.code();
                true
            }
            _ => false,
        });
        assert!(done, "the watch did not exit: {}", self.output());
        code
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        // A watch that already exited makes both calls fail; that is fine.
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// Runs `teamwire <args>` with a cleared environment: no host, no export
/// endpoint, and none of the member identity variables of the session that
/// runs the tests, which would make the binary refuse the lead's command.
fn teamwire(bed: &Bed, args: &[&str]) -> Run {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TEAMWIRE_HOME", bed.home.path())
        .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
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
    assert_eq!(printed(&run.out), [["WATCHING 2 members"]]);
    // Nothing is left behind: the lease is released.
    assert!(bed.store.read(|tx| tx.watch_lease()).unwrap().is_none());
}

/// WAT-20: the first poll says `WATCHING` and holds `TEAM-COMPLETE` back,
/// so a finished team takes two polls and two lines to end the watch.
#[test]
fn a_finished_team_ends_the_watch_on_its_second_poll_without_once() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.add("web");
    bed.delivered("api", CompletionStatus::Done);
    bed.delivered("web", CompletionStatus::Failed);
    let run = teamwire(&bed, &["watch", "alpha", "--interval", "1"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    let complete = format!("TEAM-COMPLETE 1/2 done; not done: web failed{NO_CLOSER}");
    assert_eq!(
        printed(&run.out),
        [vec!["WATCHING 2 members"], vec![complete.as_str()]]
    );
}

/// WAT-20: `--once` reads the team's state in a single poll and prints what
/// it found, for the scripts and tests that read it that way.
#[test]
fn once_on_a_finished_team_prints_team_complete_in_its_one_poll() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.delivered("api", CompletionStatus::Done);
    let run = teamwire(&bed, &["watch", "alpha", "--once"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    let complete = format!("TEAM-COMPLETE 1/1 done{NO_CLOSER}");
    assert_eq!(
        printed(&run.out),
        [["WATCHING 1 members", complete.as_str()]]
    );
}

/// WAT-20, the failure of team `fb-2026-11-03`: a watch read a settled
/// roster 235 ms before `launch --add` wrote a new member's row, printed
/// `TEAM-COMPLETE`, exited, and that member ran unwatched for 3.5 minutes.
/// The first poll now holds `TEAM-COMPLETE` back, which leaves a whole
/// interval for the row to be written, and the running watch picks it up.
#[test]
fn a_member_added_after_the_first_poll_is_watched_by_the_running_watch() {
    let mut bed = Bed::new();
    // The binary reads the real clock and the fake clock stands in 2026, so
    // give the members a deadline that has not passed.
    bed.add_with_deadline("api", 100_000_000);
    bed.delivered("api", CompletionStatus::Done);
    let mut watch = Watching::start(&bed, &["--interval", "2"]);
    watch.wait_for("WATCHING 1 members");
    let first_poll = Watching::heartbeat(&bed);
    // `launch --add` writes the new member while the watch waits, as the
    // lead's launch did in `fb`.
    bed.add_with_deadline("web", 100_000_000);
    // Two polls have finished, so one of them read the roster after the new
    // member's row was written.
    Watching::wait_for_two_polls_after(&bed, first_poll);
    assert!(
        !watch.output().contains("TEAM-COMPLETE"),
        "the team waits for the added member: {}",
        watch.output()
    );
    bed.delivered("web", CompletionStatus::Done);
    watch.wait_for("TEAM-COMPLETE 2/2 done");
    assert_eq!(watch.exit_code(), Some(0), "{}", watch.output());
}

#[test]
fn a_second_watch_is_refused_and_names_the_first() {
    let mut bed = Bed::new();
    bed.add("api");
    // This test process holds the lease, and it is alive.
    let me = Pid::new(std::process::id());
    // The binary reads the real clock, and a lease whose heartbeat is old is
    // taken over, so the lease is stamped with the real time.
    let at = teamwire_platform::SystemClock.now();
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
fn a_closed_team_ends_the_watch_with_one_team_closed_line() {
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
    assert_eq!(printed(&run.out), [["TEAM-CLOSED"]], "{}", run.out);
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
    // A longer interval would let the lease look dead between two polls.
    let run = teamwire(&bed, &["watch", "alpha", "--interval", "301", "--once"]);
    assert_eq!(run.code, 3, "{}", run.err);
    assert!(run.err.contains("300"), "{}", run.err);
    let run = teamwire(&bed, &["watch", "alpha", "--interval", "300", "--once"]);
    assert_eq!(run.code, 0, "{}", run.err);
}

#[test]
fn a_store_from_a_newer_binary_exits_six() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.store
        .write(|tx| {
            tx.connection()
                .execute_batch(&format!(
                    "PRAGMA user_version = {}",
                    teamwire_store::SCHEMA_VERSION + 1
                ))
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
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:1")
        .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let complete = format!("TEAM-COMPLETE 1/1 done{NO_CLOSER}");
    assert_eq!(
        printed(&String::from_utf8(out.stdout).unwrap()),
        [vec!["WATCHING 1 members"], vec![complete.as_str()]]
    );
}

/// A team whose lead may be a live `fake-claude` session in a `fake-herdr`
/// pane, and one started member with no session yet.
struct LeadRig {
    herdr: teamwire_fakes::FakeHerdrEnv,
    home: teamwire_fakes::TestHome,
}

impl LeadRig {
    fn new(lead_live: bool) -> Self {
        use teamwire_core::{
            HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode, Role, SessionId,
            SessionKind, TeamName,
        };
        let mut herdr = teamwire_fakes::FakeHerdrEnv::new().unwrap();
        herdr
            .set_script(&teamwire_fakes::ClaudeScript::default())
            .unwrap();
        let home = teamwire_fakes::TestHome::new().unwrap();
        if lead_live {
            let ws = herdr
                .run(&[
                    "workspace",
                    "create",
                    "--cwd",
                    "/tmp",
                    "--label",
                    "lead",
                    "--no-focus",
                ])
                .unwrap();
            let pane = ws
                .result()
                .and_then(|r| r.pointer("/root_pane/pane_id")?.as_str().map(str::to_owned))
                .unwrap();
            let settings = herdr
                .write_file("lead-settings.json", r#"{"env":{},"hooks":{}}"#)
                .unwrap();
            let started = herdr
                .run(&[
                    "agent",
                    "start",
                    "lead-agent",
                    "--kind",
                    "claude",
                    "--pane",
                    &pane,
                    "--timeout",
                    "20000",
                    "--",
                    "-n",
                    "lead-agent",
                    "--session-id",
                    "lead-1",
                    "--settings",
                    &settings.display().to_string(),
                ])
                .unwrap();
            assert_eq!(started.code, Some(0), "{started:?}");
        }
        let at = teamwire_platform::SystemClock.now();
        let mut store =
            teamwire_store::Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at,
                    &teamwire_store::NewTeam {
                        name: TeamName::parse("alpha").unwrap(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse("lead-1").unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )?;
                let api = Role::parse("api").unwrap();
                tx.insert_member(
                    at,
                    &teamwire_store::NewMember {
                        role: api.clone(),
                        title: "api".to_owned(),
                        kind: MemberKind::Implementer,
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
                        deadline_s: 1_000_000_000,
                    },
                )?;
                tx.set_member_state(at, &api, MemberState::Started, &serde_json::json!({}))
            })
            .unwrap();
        Self { herdr, home }
    }

    fn watch(&self, args: &[&str]) -> Run {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .args(["watch", "alpha", "--once"])
            .args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .output()
            .unwrap();
        Run {
            code: out.status.code().unwrap(),
            out: String::from_utf8(out.stdout).unwrap(),
            err: String::from_utf8(out.stderr).unwrap(),
        }
    }

    /// `teamwire pr-watch -R o/r -i 1 7 <args>` with a fake `gh` first on
    /// `PATH`. The fake says pull request 7 is merged, so the watch prints
    /// two lines and exits. `extra` is set on top of the cleared environment.
    fn pr_watch(&self, args: &[&str], extra: &[(&str, &str)]) -> Run {
        use std::os::unix::fs::PermissionsExt as _;
        let bin = self.home.path().join("gh-bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncase \"$2\" in\n  checks) echo '[]' ;;\n  view) echo \
             '{\"state\":\"MERGED\",\"baseRefName\":\"main\"}' ;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut env = self.herdr.env();
        for (key, value) in &mut env {
            if key == "PATH" {
                *value = format!("{}:{value}", bin.display());
            }
        }
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .args(["pr-watch", "-R", "o/r", "-i", "1"])
            .args(args)
            .arg("7")
            .env_clear()
            .envs(env)
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .envs(extra.iter().copied())
            .output()
            .unwrap();
        Run {
            code: out.status.code().unwrap(),
            out: String::from_utf8(out.stdout).unwrap(),
            err: String::from_utf8(out.stderr).unwrap(),
        }
    }

    /// The arguments of every `agent prompt` call, joined.
    fn typed(&self) -> Vec<String> {
        self.herdr
            .calls()
            .into_iter()
            .filter(|c| c.command() == "agent prompt")
            .map(|c| c.argv.join(" "))
            .collect()
    }
}

/// INV-8: the watch prints its lines and never types into a session, even
/// with a live lead in a pane.
#[test]
fn the_watch_types_nothing_into_the_leads_pane() {
    let rig = LeadRig::new(true);
    let run = rig.watch(&[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(rig.typed().is_empty());
}

/// ADR 0055: no watch types its lines into the lead's pane any more, so the
/// command line refuses `--deliver` as it refuses any unknown flag, and the
/// watch never starts.
#[test]
fn watch_refuses_deliver_as_a_usage_error() {
    let rig = LeadRig::new(true);
    let run = rig.watch(&["--deliver"]);
    assert_eq!(run.code, 3, "{}{}", run.out, run.err);
    assert!(run.err.contains("--deliver"), "{}", run.err);
    assert!(run.out.is_empty(), "{}", run.out);
    assert!(rig.typed().is_empty());
}

/// ADR 0055: `pr-watch --deliver <team>` is refused the same way.
#[test]
fn pr_watch_refuses_deliver_as_a_usage_error() {
    let rig = LeadRig::new(true);
    let run = rig.pr_watch(&["--deliver", "alpha"], &[]);
    assert_eq!(run.code, 3, "{}{}", run.out, run.err);
    assert!(run.err.contains("--deliver"), "{}", run.err);
    assert!(run.out.is_empty(), "{}", run.out);
    assert!(rig.typed().is_empty());
}

/// pr-watch prints its lines without a team prefix and types nothing, even
/// with a live lead in a pane.
#[test]
fn pr_watch_prints_its_lines_and_types_nothing() {
    let rig = LeadRig::new(true);
    let run = rig.pr_watch(&[], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(run.out, "WATCHING o/r#7 (poll 1s)\no/r#7 MERGED\n");
    assert!(rig.typed().is_empty());
}

#[test]
fn a_watch_that_cannot_take_the_lease_leaves_a_store_error_row() {
    let mut bed = Bed::new();
    bed.add("api");
    // The first poll records its own failures, so the failure has to come
    // before it, in the write that takes the lease. A function that does not
    // exist fails the insert with an `io` error.
    bed.store
        .write(|tx| {
            tx.connection()
                .execute_batch(
                    "CREATE TRIGGER no_lease BEFORE INSERT ON watch_lease
                     BEGIN SELECT no_such_function(); END",
                )
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let run = teamwire(&bed, &["watch", "alpha", "--once"]);
    assert_eq!(run.code, 6, "{}{}", run.out, run.err);
    let errors: Vec<serde_json::Value> = bed
        .store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == teamwire_core::EventType::StoreError)
        .map(|e| e.data)
        .collect();
    assert_eq!(
        errors,
        [serde_json::json!({ "op": "watch", "code": "io" })],
        "{}{}",
        run.out,
        run.err
    );
}
