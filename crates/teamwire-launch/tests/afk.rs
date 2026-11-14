//! AFK teams in launch: members join a team that has none yet, a spent
//! budget turns away new implementers only, and a member below the AFK
//! model table gets a warning. Each refused member writes `member.refused`,
//! as one refused because its team is closing does.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "an integration test unwraps and indexes; a failure there fails the test"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use std::path::Path;
use std::time::Duration;

use common::{GitPlatform, Rig, member_row, pane_member, team_spec, with, worktree_member};
use serde_json::json;
use teamwire_core::{
    Clock, Command, Event, EventType, FileLock, HarnessId, HostId, LeadRoute, MemberState, Pid,
    Platform, PlatformError, RunOutput, SessionId, SessionKind, SleepHold, TeamName,
};
use teamwire_fakes::{FakeClock, FakeHarness};
use teamwire_launch::LaunchOutcome;
use teamwire_store::{Ctx, NewTeam, Store};

/// An AFK team with no members whose budget ends `budget` after the rig's now.
fn afk_team(rig: &Rig, team: &str, budget: Duration) {
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(rig.home.path(), &name).unwrap();
    let at = rig.clock.now();
    store
        .write(|tx| {
            tx.create_afk_team(
                at,
                &NewTeam {
                    name: name.clone(),
                    title: "Afk run".to_owned(),
                    lead_harness: HarnessId::from_static(FakeHarness::ID),
                    lead_host: HostId::from_static(teamwire_fakes::FakeHost::ID),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((
                        SessionId::parse("lead-session").unwrap(),
                        SessionKind::Interactive,
                    )),
                },
                at.checked_add(budget).unwrap(),
                &json!({}),
            )
            .map(|_| ())
        })
        .unwrap();
}

fn add(rig: &Rig, children: &[serde_json::Value]) -> (teamwire_launch::LaunchReport, Store) {
    let plan = rig.validate(&team_spec("run", children), true).unwrap();
    rig.launch("run", plan)
}

#[test]
fn members_join_an_afk_team_that_has_none() {
    let rig = Rig::new();
    afk_team(&rig, "run", Duration::from_secs(3600));
    let (report, store) = add(&rig, &[pane_member("scout", "Scout", rig.work.path())]);
    assert_eq!(report.members[0].outcome, LaunchOutcome::Started);
    assert!(member_row(&store, "scout").is_some());
}

#[test]
fn past_the_budget_an_implementer_is_refused_and_every_other_kind_starts() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    afk_team(&rig, "run", Duration::from_secs(3600));
    rig.clock.advance(Duration::from_secs(3601));
    let reviewer = json!({"role": "rev", "title": "Rev", "kind": "reviewer", "mode": "pane",
                          "cwd": rig.work.path(), "task": "review"});
    let closer = json!({"role": "clo", "title": "Clo", "kind": "closer", "mode": "pane",
                        "cwd": rig.work.path(), "task": "close"});
    let (report, store) = add(
        &rig,
        &[
            worktree_member("api", "API", &repo),
            pane_member("scout", "Scout", rig.work.path()),
            reviewer,
            closer,
        ],
    );
    let outcomes: Vec<_> = report.members.iter().map(|m| &m.outcome).collect();
    let LaunchOutcome::Refused(why) = outcomes[0] else {
        panic!("the implementer was not refused: {outcomes:?}");
    };
    assert!(why.starts_with("afk_budget_spent"), "{why}");
    assert!(outcomes[1..].iter().all(|o| **o == LaunchOutcome::Started));
    assert!(
        member_row(&store, "api").is_none(),
        "a refused member leaves no row"
    );
}

fn refused_events(store: &Store) -> Vec<Event> {
    store
        .read(|tx| tx.events_of_type(EventType::MemberRefused))
        .unwrap()
        .items
}

#[test]
fn past_the_budget_a_refused_implementer_writes_one_member_refused_event() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    afk_team(&rig, "run", Duration::from_secs(3600));
    rig.clock.advance(Duration::from_secs(3601));
    let (report, store) = add(
        &rig,
        &[
            worktree_member("api", "API", &repo),
            pane_member("scout", "Scout", rig.work.path()),
        ],
    );
    assert!(matches!(
        report.members[0].outcome,
        LaunchOutcome::Refused(_)
    ));
    assert!(member_row(&store, "api").is_none());
    let refused = refused_events(&store);
    assert_eq!(refused.len(), 1, "{refused:?}");
    let e = &refused[0];
    assert_eq!(e.role.as_ref().unwrap().as_str(), "api");
    assert_eq!(e.data["reason_code"], "afk_budget_spent");
    assert_eq!(e.data["requested"], 2);
    // The same launch id as the member that started, so the launch span
    // counts both.
    let started = store
        .read(|tx| tx.events_of_type(EventType::MemberStarted))
        .unwrap()
        .items;
    assert_eq!(e.data["launch"], started[0].data["launch"]);
}

/// The rig's platform, except that the setup command moves the clock forward
/// by `by` while it runs, as a long setup would.
#[derive(Debug)]
struct SlowSetup<'a> {
    inner: &'a GitPlatform,
    clock: FakeClock,
    by: Duration,
}

impl Platform for SlowSetup<'_> {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        if cmd.args.iter().any(|a| a == "teamwire-setup") {
            self.clock.advance(self.by);
        }
        self.inner.run(cmd, timeout)
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
        self.inner.write_atomic(path, bytes)
    }
    fn lock_file(&self, path: &Path, wait: Duration) -> Result<FileLock, PlatformError> {
        self.inner.lock_file(path, wait)
    }
    fn sleep_hold(&self) -> Option<&dyn SleepHold> {
        self.inner.sleep_hold()
    }
    fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError> {
        self.inner.free_bytes(path)
    }
    fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError> {
        self.inner.on_network_fs(path)
    }
}

#[test]
fn an_implementer_whose_setup_runs_past_the_budget_is_refused_before_spawn() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    afk_team(&rig, "run", Duration::from_secs(3600));
    rig.clock.advance(Duration::from_secs(3500));
    let member = with(worktree_member("api", "API", &repo), "setup", json!("true"));
    let plan = rig.validate(&team_spec("run", &[member]), true).unwrap();
    let platform = SlowSetup {
        inner: &rig.platform,
        clock: rig.clock.clone(),
        by: Duration::from_secs(200),
    };
    let mut store = Store::open(rig.home.path(), &TeamName::parse("run").unwrap()).unwrap();
    let report = teamwire_launch::launch(
        &mut Ctx {
            home: rig.home.path(),
            store: &mut store,
            registry: &rig.registry,
            platform: &platform,
            clock: &rig.clock as &dyn Clock,
            config: &rig.config,
        },
        plan,
    );

    let LaunchOutcome::Refused(why) = &report.members[0].outcome else {
        panic!("the implementer was not refused: {:?}", report.members);
    };
    assert!(why.starts_with("afk_budget_spent"), "{why}");
    assert_eq!(
        rig.host.log().count_named("spawn"),
        0,
        "an agent was started"
    );
    let refused = refused_events(&store);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["reason_code"], "afk_budget_spent");
    assert!(refused[0].data["steps"]["setup"].is_number());
    // The row stays `failed`, as a failed launch leaves it, with no
    // `member.failed` beside the refusal.
    let row = member_row(&store, "api").unwrap();
    assert_eq!(row.state, MemberState::Failed);
    assert!(!row.cwd.exists(), "the worktree was not undone");
    let failed = store
        .read(|tx| tx.events_of_type(EventType::MemberFailed))
        .unwrap()
        .items;
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn a_member_refused_by_a_closing_team_writes_member_refused() {
    let rig = Rig::new();
    common::seed_team(&rig, "run", |_, _| Ok(()));
    let plan = rig
        .validate(
            &team_spec("run", &[pane_member("scout", "Scout", rig.work.path())]),
            true,
        )
        .unwrap();
    let mut other =
        Store::open_existing(rig.home.path(), &TeamName::parse("run").unwrap()).unwrap();
    other
        .write(|tx| tx.begin_close(rig.clock.now(), Pid::new(1)))
        .unwrap();
    let (report, store) = rig.launch("run", plan);
    assert!(matches!(
        report.members[0].outcome,
        LaunchOutcome::Refused(_)
    ));
    assert!(member_row(&store, "scout").is_none());
    let refused = refused_events(&store);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["reason_code"], "team_closed");
}

#[test]
fn before_the_budget_ends_an_implementer_starts() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    afk_team(&rig, "run", Duration::from_secs(3600));
    rig.clock.advance(Duration::from_secs(3599));
    let (report, _) = add(&rig, &[worktree_member("api", "API", &repo)]);
    assert_eq!(report.members[0].outcome, LaunchOutcome::Started);
}

#[test]
fn an_ordinary_team_has_no_budget() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    common::seed_team(&rig, "run", |_, _| Ok(()));
    rig.clock.advance(Duration::from_secs(1_000_000));
    let (report, _) = add(&rig, &[worktree_member("api", "API", &repo)]);
    assert_eq!(report.members[0].outcome, LaunchOutcome::Started);
}

fn model_member(
    role: &str,
    kind: &str,
    model: Option<&str>,
    effort: Option<&str>,
    cwd: &std::path::Path,
) -> serde_json::Value {
    let mut m = json!({"role": role, "title": role, "kind": kind, "mode": "pane",
                       "cwd": cwd, "task": "x"});
    if let Some(model) = model {
        m["model"] = json!(model);
    }
    if let Some(effort) = effort {
        m["effort"] = json!(effort);
    }
    m
}

fn warnings_for(rig: &Rig, team_afk: bool, children: &[serde_json::Value]) -> Vec<String> {
    if team_afk {
        afk_team(rig, "run", Duration::from_secs(3600));
    } else {
        common::seed_team(rig, "run", |_, _| Ok(()));
    }
    rig.validate(&team_spec("run", children), true)
        .unwrap()
        .warnings
}

#[test]
fn a_member_below_the_afk_table_gets_one_warning_each() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let warnings = warnings_for(
        &rig,
        true,
        &[
            model_member("a", "investigator", Some("sonnet"), Some("high"), cwd),
            model_member("b", "reviewer", Some("opus"), Some("high"), cwd),
            model_member("c", "closer", None, None, cwd),
            model_member("d", "investigator", Some("opus"), Some("max"), cwd),
            model_member("e", "reviewer", Some("opus"), Some("xhigh"), cwd),
            model_member("f", "investigator", Some("other-model"), Some("high"), cwd),
        ],
    );
    let afk: Vec<_> = warnings.iter().filter(|w| w.contains("AFK")).collect();
    assert_eq!(afk.len(), 3, "{warnings:?}");
    for (w, role) in afk.iter().zip(["a", "b", "c"]) {
        assert!(w.contains(&format!("member {role} ")), "{w}");
    }
}

#[test]
fn an_ordinary_team_gets_no_afk_warning() {
    let rig = Rig::new();
    let warnings = warnings_for(
        &rig,
        false,
        &[model_member(
            "a",
            "investigator",
            Some("haiku"),
            Some("low"),
            rig.work.path(),
        )],
    );
    assert!(warnings.iter().all(|w| !w.contains("AFK")), "{warnings:?}");
}
