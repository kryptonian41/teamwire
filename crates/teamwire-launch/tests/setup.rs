//! Setup runs before the agent exists; a failed setup undoes the member.

#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, git, member_row, one_worktree_team, role_events};
use serde_json::json;
use teamwire_core::MemberState;
use teamwire_launch::LaunchOutcome;

#[test]
fn failed_setup_undoes_and_role_reusable() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let failing = common::with(spec.clone(), "setup", json!("echo boom; exit 3"));
    let plan = rig.validate(&failing, false).unwrap();
    let (report, store) = rig.launch("demo", plan);

    let outcome = &report.members.first().unwrap().outcome;
    assert!(matches!(outcome, LaunchOutcome::Failed(_)), "{report:?}");
    let reason = format!("{outcome:?}");
    assert!(reason.contains("setup exited 3"), "{reason}");
    assert!(
        reason.contains("worktree") && reason.contains("removed"),
        "{reason}"
    );
    assert!(reason.contains("--add"), "{reason}");
    // Undone: no checkout, no branch, no agent; the log is kept.
    assert!(!repo.join(".worktrees/api").exists());
    assert!(!git(&repo, &["worktree", "list", "--porcelain"]).contains(".worktrees/api"));
    assert_eq!(git(&repo, &["branch", "--list", "api"]), "");
    assert_eq!(rig.host.log().count_named("spawn"), 0);
    let log = std::fs::read_to_string(rig.home.path().join("teams/demo/setup/api.log")).unwrap();
    assert!(log.contains("boom") && log.contains("[exit 3]"));
    let row = member_row(&store, "api").unwrap();
    assert_eq!(row.state, MemberState::Failed);
    let events = role_events(&store, "api");
    assert_eq!(events.last().unwrap().0, "member.failed");
    // A metric label comes from the code, never from the free text (M8).
    let code = events
        .last()
        .unwrap()
        .1
        .get("reason_code")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(code, "setup");
    assert!(
        (1..=32).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
        "{code}"
    );
    drop(store);

    // The same role, launched again with a corrected spec.
    let plan = rig.validate(&spec, true).unwrap();
    assert!(plan.members.first().unwrap().relaunch);
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let kinds: Vec<String> = role_events(&store, "api")
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        kinds,
        [
            "member.creating",
            "member.failed",
            "member.replaced",
            "member.creating",
            "member.started"
        ]
    );
    assert!(repo.join(".worktrees/api/README.md").is_file());
    assert_eq!(
        member_row(&store, "api").unwrap().state,
        MemberState::Started
    );
}

#[test]
fn setup_gets_the_directory_as_first_argument_and_the_team_variables() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let script = r#"printf '%s|%s|%s|%s|%s' "$1" "$TEAMWIRE_TEAM" "$TEAMWIRE_ROLE" "$TEAMWIRE_HOME" "$TEAMWIRE_CHILD_CWD" > setup-out.txt; pwd >> setup-out.txt"#;
    let spec = common::with(spec, "setup", json!(script));
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let dir = repo.join(".worktrees/api");
    let out = std::fs::read_to_string(dir.join("setup-out.txt")).unwrap();
    let want = format!(
        "{d}|demo|api|{h}|{d}",
        d = dir.display(),
        h = rig.home.path().display()
    );
    assert!(out.starts_with(&want), "{out} vs {want}");
}

#[test]
fn a_pane_member_does_not_get_the_team_setup() {
    let rig = Rig::new();
    let spec = common::team_spec(
        "demo",
        &[common::pane_member("a", "Alpha", rig.work.path())],
    );
    let spec = common::with(spec, "setup", json!("exit 9"));
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    assert!(
        !rig.home.path().join("teams/demo/setup/a.log").exists(),
        "setup never ran"
    );
}

#[test]
fn failed_setup_that_wrote_a_file_reports_leftover_worktree() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let failing = common::with(spec.clone(), "setup", json!("touch made.txt; exit 3"));
    let plan = rig.validate(&failing, false).unwrap();
    let (report, store) = rig.launch("demo", plan);

    let outcome = &report.members.first().unwrap().outcome;
    let reason = format!("{outcome:?}");
    assert!(reason.contains("setup exited 3"), "{reason}");
    assert!(
        reason.contains("made.txt"),
        "the leftover path is named: {reason}"
    );
    assert!(
        reason.contains("git worktree remove"),
        "the manual step: {reason}"
    );
    assert!(
        !reason.contains("--add"),
        "no advice that cannot work: {reason}"
    );
    assert!(
        repo.join(".worktrees/api/made.txt").is_file(),
        "nothing was forced"
    );
    assert_eq!(
        member_row(&store, "api").unwrap().state,
        MemberState::Failed
    );
    drop(store);

    // The same role again: the worktree is reused, not refused as `path_exists`.
    let plan = rig.validate(&spec, true).unwrap();
    assert!(
        plan.members
            .first()
            .unwrap()
            .worktree
            .as_ref()
            .unwrap()
            .reuse
    );
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started,
        "{report:?}"
    );
    assert_eq!(
        member_row(&store, "api").unwrap().state,
        MemberState::Started
    );
}

#[test]
fn a_modified_tracked_file_is_named_in_full() {
    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let failing = common::with(spec, "setup", json!("echo more >> README.md; exit 3"));
    let plan = rig.validate(&failing, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    let reason = format!("{:?}", report.members.first().unwrap().outcome);
    assert!(reason.contains("still holds README.md"), "{reason}");
}

const MAKES_FILES: &str = "touch .setup-marker; mkdir -p cache/deep; touch cache/deep/x";

#[test]
fn files_setup_created_are_ignored_and_named() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!(MAKES_FILES)), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let dir = repo.join(".worktrees/api");
    // Close asks `git status --porcelain --untracked-files=normal` and
    // refuses a worktree that prints anything (teamwire-close/src/facts.rs).
    assert_eq!(
        git(&dir, &["status", "--porcelain", "--untracked-files=normal"]),
        ""
    );
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|l| l == "/.setup-marker"), "{exclude}");
    assert!(exclude.lines().any(|l| l == "/cache/deep/x"), "{exclude}");
    let note = report
        .warnings
        .iter()
        .find(|w| w.contains(".setup-marker"))
        .unwrap();
    assert!(
        note.contains("/cache/deep/x") && note.contains("info/exclude"),
        "{note}"
    );
}

#[test]
fn a_second_member_adds_no_duplicate_lines_and_old_lines_stay() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let exclude = repo.join(".git/info/exclude");
    common::write(&exclude, "# mine\n*.log");
    let spec = common::team_spec(
        "demo",
        &[
            common::worktree_member("api", "API", &repo),
            common::worktree_member("web", "Web", &repo),
        ],
    );
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!(MAKES_FILES)), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    assert!(
        report
            .members
            .iter()
            .all(|m| m.outcome == LaunchOutcome::Started),
        "{report:?}"
    );
    let text = std::fs::read_to_string(&exclude).unwrap();
    assert!(text.starts_with("# mine\n*.log\n"), "{text}");
    for line in ["/.setup-marker", "/cache/deep/x", "/TASK.md"] {
        assert_eq!(text.lines().filter(|l| *l == line).count(), 1, "{text}");
    }
}

#[test]
fn setup_that_writes_nothing_changes_no_exclude_file() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    let before = std::fs::read_to_string(&exclude).ok();
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!("true")), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    assert!(
        report.warnings.iter().all(|w| !w.contains("setup created")),
        "{:?}",
        report.warnings
    );
    let after = std::fs::read_to_string(&exclude).unwrap();
    // Only the brief is added, as before.
    assert_eq!(after, format!("{}/TASK.md\n", before.unwrap_or_default()));
}

#[test]
fn a_file_name_with_a_newline_is_named_and_never_written() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let script = "touch 'a\nb' 'sp ace' '#hash'";
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!(script)), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(
        !exclude.contains("a\nb") && !exclude.lines().any(|l| l == "/a" || l == "b"),
        "{exclude}"
    );
    assert!(exclude.lines().any(|l| l == "/sp ace"), "{exclude}");
    assert!(exclude.lines().any(|l| l == "/\\#hash"), "{exclude}");
    assert!(
        report.warnings.iter().any(|w| w.contains("a\\nb")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_file_written_later_in_a_setup_made_directory_stays_visible() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let spec = common::with(spec, "setup", json!("mkdir -p gen; touch gen/cache"));
    let plan = rig.validate(&spec, false).unwrap();
    let (_report, _store) = rig.launch("demo", plan);
    let dir = repo.join(".worktrees/api");
    common::write(&dir.join("gen/work.rs"), "fn main() {}\n");
    assert_eq!(
        git(&dir, &["status", "--porcelain", "--untracked-files=normal"]),
        "?? gen/"
    );
}

#[test]
fn a_member_that_fails_after_setup_takes_its_exclude_lines_back() {
    use teamwire_core::{HostError, HostErrorCode};
    use teamwire_fakes::Fault;
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    common::write(&exclude, "# mine\n*.log\n");
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::Other, "boom")),
        1,
    );
    let plan = rig
        .validate(&common::with(spec, "setup", json!("touch made.txt")), false)
        .unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert!(
        matches!(
            report.members.first().unwrap().outcome,
            LaunchOutcome::Failed(_)
        ),
        "{report:?}"
    );
    let text = std::fs::read_to_string(&exclude).unwrap();
    assert_eq!(text, "# mine\n*.log\n");
}

#[test]
fn the_exclude_note_agrees_in_number() {
    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let one = rig
        .validate(
            &common::with(spec.clone(), "setup", json!("touch a.txt")),
            false,
        )
        .unwrap();
    let (report, _store) = rig.launch("demo", one);
    let note = report
        .warnings
        .iter()
        .find(|w| w.contains("a.txt"))
        .unwrap();
    assert!(note.contains("a.txt; launch added it to"), "{note}");

    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let two = rig
        .validate(
            &common::with(spec, "setup", json!("touch a.txt b.txt")),
            false,
        )
        .unwrap();
    let (report, _store) = rig.launch("demo", two);
    let note = report
        .warnings
        .iter()
        .find(|w| w.contains("a.txt"))
        .unwrap();
    assert!(
        note.contains("/a.txt, /b.txt; launch added them to"),
        "{note}"
    );
}

#[test]
fn a_deleted_branch_is_cleared_from_the_member_row() {
    use teamwire_core::{HostError, HostErrorCode};
    use teamwire_fakes::Fault;
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::Other, "boom")),
        1,
    );
    let plan = rig.validate(&spec, false).unwrap();
    let (_report, store) = rig.launch("demo", plan);
    assert!(
        git(&repo, &["branch", "--list", "api"]).trim().is_empty(),
        "the branch was deleted"
    );
    let row = member_row(&store, "api").unwrap();
    assert_eq!(row.state, MemberState::Failed);
    assert_eq!(row.branch, None);
}

/// Takes team `demo`'s close lock through a second connection, as
/// `teamwire close` does while a launch runs.
fn take_close_lock(home: &std::path::Path) {
    let team = teamwire_core::TeamName::parse("demo").unwrap();
    let mut store = teamwire_store::Store::open(home, &team).unwrap();
    let now = teamwire_core::Clock::now(&teamwire_fakes::FakeClock::new());
    let taken = store
        .write(|tx| tx.begin_close(now, teamwire_core::Pid::new(1)))
        .unwrap();
    assert!(taken, "the team was open");
}

/// A host that takes the close lock once, right before its first call named
/// `on`, then does what the fake does.
struct LockingHost {
    inner: std::sync::Arc<teamwire_fakes::FakeHost>,
    on: &'static str,
    home: std::path::PathBuf,
    taken: std::sync::atomic::AtomicBool,
}

impl LockingHost {
    fn before(&self, op: &str) {
        if op == self.on && !self.taken.swap(true, std::sync::atomic::Ordering::SeqCst) {
            take_close_lock(&self.home);
        }
    }
}

mod locking_host {
    use std::time::Duration;

    use teamwire_core::{
        Address, Host, HostCaps, HostError, HostId, Key, Occupant, OneLine, Place, PlaceState,
        SpawnRequest,
    };

    impl Host for super::LockingHost {
        fn id(&self) -> HostId {
            self.inner.id()
        }
        fn capabilities(&self) -> HostCaps {
            self.inner.capabilities()
        }
        fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
            self.before("spawn");
            self.inner.spawn(req, timeout)
        }
        fn submit(
            &self,
            place: &Place,
            text: &OneLine,
            timeout: Duration,
        ) -> Result<(), HostError> {
            self.before("submit");
            self.inner.submit(place, text, timeout)
        }
        fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
            self.before("state");
            self.inner.state(place, timeout)
        }
        fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
            self.before("occupant");
            self.inner.occupant(place, timeout)
        }
        fn read_tail(
            &self,
            place: &Place,
            lines: u16,
            timeout: Duration,
        ) -> Result<String, HostError> {
            self.before("read_tail");
            self.inner.read_tail(place, lines, timeout)
        }
        fn send_keys(
            &self,
            place: &Place,
            keys: &[Key],
            timeout: Duration,
        ) -> Result<(), HostError> {
            self.before("send_keys");
            self.inner.send_keys(place, keys, timeout)
        }
        fn close(
            &self,
            place: &Place,
            expected: &Address,
            timeout: Duration,
        ) -> Result<(), HostError> {
            self.before("close");
            self.inner.close(place, expected, timeout)
        }
        fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
            self.before("list");
            self.inner.list(timeout)
        }
        fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
            self.before("live_agent_names");
            self.inner.live_agent_names(timeout)
        }
        fn place_env_var(&self) -> Option<&'static str> {
            self.inner.place_env_var()
        }
        fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
            self.inner.place_from_env(read)
        }
    }
}

/// Puts a `LockingHost` around the rig's fake host.
fn lock_on(rig: &mut Rig, on: &'static str) {
    let host = std::sync::Arc::new(LockingHost {
        inner: rig.host.clone(),
        on,
        home: rig.home.path().to_path_buf(),
        taken: std::sync::atomic::AtomicBool::new(false),
    });
    let mut registry = teamwire_core::Registry::new();
    registry.add_harness(rig.harness.clone()).unwrap();
    registry.add_host(host).unwrap();
    rig.registry = registry;
}

/// The launch was refused with `team_closed`, its member is `failed` with
/// `member.refused` as its last event, and no agent of it is left running.
fn assert_refused_by_close(
    rig: &Rig,
    report: &teamwire_launch::LaunchReport,
    store: &teamwire_store::Store,
) {
    let outcome = &report.members.first().unwrap().outcome;
    assert!(
        matches!(outcome, LaunchOutcome::Refused(why) if why.starts_with("team_closed:")),
        "{outcome:?}"
    );
    assert_eq!(member_row(store, "api").unwrap().state, MemberState::Failed);
    let events = role_events(store, "api");
    assert_eq!(events.last().unwrap().0, "member.refused", "{events:?}");
    let live = teamwire_core::Host::live_agent_names(&*rig.host, std::time::Duration::from_secs(5))
        .unwrap();
    assert!(!live.iter().any(|n| n == "tw_demo_api"), "{live:?}");
}

/// A close takes the lock while the agent starts: launch does not record the
/// place, closes it, and refuses.
#[test]
fn a_close_during_spawn_refuses_the_launch_and_closes_the_agent() {
    let mut rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    lock_on(&mut rig, "spawn");
    let plan = rig.validate(&spec, false).unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert_refused_by_close(&rig, &report, &store);
    assert_eq!(rig.host.log().count_named("spawn"), 1);
    assert_eq!(member_row(&store, "api").unwrap().place, None);
    assert!(!repo.join(".worktrees/api").exists());
}

/// A close takes the lock during the trust step: launch does not mark the
/// member started.
#[test]
fn a_close_during_the_trust_step_refuses_the_launch() {
    let mut rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    lock_on(&mut rig, "read_tail");
    let plan = rig.validate(&spec, false).unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert_refused_by_close(&rig, &report, &store);
    let kinds: Vec<String> = role_events(&store, "api")
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(kinds, ["member.creating", "member.refused"]);
}

/// Runs `run` like the rig's platform and takes the close lock after the
/// first setup script.
#[derive(Debug)]
struct LockingSetup<'a> {
    inner: &'a common::GitPlatform,
    home: std::path::PathBuf,
}

impl teamwire_core::Platform for LockingSetup<'_> {
    fn pid_alive(&self, pid: teamwire_core::Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(
        &self,
        pid: teamwire_core::Pid,
        var: &str,
    ) -> Result<Option<String>, teamwire_core::PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(
        &self,
        cmd: &teamwire_core::Command,
        timeout: std::time::Duration,
    ) -> Result<teamwire_core::RunOutput, teamwire_core::PlatformError> {
        let out = self.inner.run(cmd, timeout);
        if cmd.program == "sh" {
            take_close_lock(&self.home);
        }
        out
    }
    fn write_atomic(
        &self,
        path: &std::path::Path,
        bytes: &[u8],
    ) -> Result<(), teamwire_core::PlatformError> {
        self.inner.write_atomic(path, bytes)
    }
    fn sleep_hold(&self) -> Option<&dyn teamwire_core::SleepHold> {
        self.inner.sleep_hold()
    }
    fn free_bytes(&self, path: &std::path::Path) -> Result<u64, teamwire_core::PlatformError> {
        self.inner.free_bytes(path)
    }
    fn on_network_fs(&self, path: &std::path::Path) -> Result<bool, teamwire_core::PlatformError> {
        self.inner.on_network_fs(path)
    }
}

/// A close takes the lock while setup runs: launch writes no contract, brief
/// or `TASK.md` and starts nothing.
#[test]
fn a_close_during_setup_refuses_before_any_file_is_written() {
    use teamwire_core::Platform;
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let plan = rig
        .validate(&common::with(spec, "setup", json!("true")), false)
        .unwrap();
    let platform = LockingSetup {
        inner: &rig.platform,
        home: rig.home.path().to_path_buf(),
    };
    let team = teamwire_core::TeamName::parse("demo").unwrap();
    let mut store = teamwire_store::Store::open(rig.home.path(), &team).unwrap();
    let report = teamwire_launch::launch(
        &mut teamwire_store::Ctx {
            home: rig.home.path(),
            store: &mut store,
            registry: &rig.registry,
            platform: &platform as &dyn Platform,
            clock: &rig.clock,
            config: &rig.config,
        },
        plan,
    );
    assert_refused_by_close(&rig, &report, &store);
    assert_eq!(rig.host.log().count_named("spawn"), 0);
    let team_dir = rig.home.path().join("teams/demo");
    assert!(!team_dir.join("contract/api.md").exists());
    assert!(!team_dir.join("briefs/api.md").exists());
    assert!(!repo.join(".worktrees/api").exists());
}

/// The lines launch added to `info/exclude` are in the `member.started` data,
/// so close can take them back when the team ends.
#[test]
fn the_added_exclude_lines_are_recorded_when_the_member_starts() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    common::write(&exclude, "# mine\n/.setup-marker\n");
    let script = "touch .setup-marker made.txt";
    let plan = rig
        .validate(&common::with(spec, "setup", json!(script)), false)
        .unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let events = role_events(&store, "api");
    let (kind, data) = events.last().unwrap();
    assert_eq!(kind, "member.started");
    let file = std::fs::canonicalize(&exclude).unwrap();
    let recorded: Vec<(std::path::PathBuf, Vec<String>)> = data["excluded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let lines = e["lines"].as_array().unwrap();
            (
                std::fs::canonicalize(e["file"].as_str().unwrap()).unwrap(),
                lines
                    .iter()
                    .map(|l| l.as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect();
    // The line that was already there is not launch's to take back.
    assert_eq!(
        recorded,
        [
            (file.clone(), vec!["/made.txt".to_owned()]),
            (file, vec!["/TASK.md".to_owned()]),
        ]
    );
}
