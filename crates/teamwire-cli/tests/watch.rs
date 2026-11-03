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
    assert_eq!(
        run.out.trim_end(),
        "[teamwire:alpha] TEAM-CLOSED",
        "{}",
        run.out
    );
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
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "[teamwire:alpha] WATCHING 1 members | [teamwire:alpha] TEAM-COMPLETE 1/1 done\n"
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

#[test]
fn deliver_types_each_printed_line_into_the_leads_pane() {
    let rig = LeadRig::new(true);
    let run = rig.watch(&["--deliver"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(run.out, "[teamwire:alpha] WATCHING 1 members\n");
    let typed = rig.typed();
    assert_eq!(typed.len(), 1, "{typed:?}");
    assert!(
        typed
            .iter()
            .all(|t| t.contains("[teamwire:alpha] WATCHING 1 members")),
        "{typed:?}"
    );
}

#[test]
fn without_deliver_nothing_is_typed() {
    let rig = LeadRig::new(true);
    let run = rig.watch(&[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(rig.typed().is_empty());
}

#[test]
fn a_line_that_cannot_be_delivered_is_still_printed() {
    let rig = LeadRig::new(false);
    let run = rig.watch(&["--deliver"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(run.out, "[teamwire:alpha] WATCHING 1 members\n");
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
