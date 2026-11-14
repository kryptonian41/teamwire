//! Whole-team close of a worktree member whose HEAD is detached (CLS-1): its
//! place is closed right before its worktree is removed, inside
//! `remove_worktrees`, so nothing can commit between the last HEAD check and
//! the removal. A member on a branch keeps its place until `close_places`.

#[path = "support/rig.rs"]
pub mod rig;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use rig::{GitPlatform, Rig, git};
use teamwire_close::{CloseOpts, CloseReport, CloseTarget, close, close_observed};
use teamwire_core::{
    Command, Host, MemberKind, Occupant, Pid, Place, Platform, PlatformError, RunOutput, SleepHold,
    Timestamp,
};
use teamwire_fakes::FakeHost;
use teamwire_store::Ctx;

/// An implementer that detached its HEAD and committed there, so the commit
/// is on no branch, tag or remote ref. Returns the commit's sha.
fn detached_commit(rig: &mut Rig, role: &str) -> String {
    let member = rig.add_worktree_member(role, MemberKind::Implementer);
    git(&member.cwd, &["checkout", "-q", "--detach"]);
    commit(&member.cwd, "work on a detached head")
}

/// A commit in `cwd`, as the agent working there makes one. Returns its sha.
fn commit(cwd: &std::path::Path, message: &str) -> String {
    git(cwd, &["commit", "-q", "--allow-empty", "-m", message]);
    git(cwd, &["rev-parse", "HEAD"])
}

/// The agent of `place`, at work in `cwd`. If its place is still open when
/// close starts `git worktree remove`, the agent commits first: the commit
/// lands after the last HEAD check and is on no ref once the worktree goes.
/// Every call then goes on to the rig's platform.
struct AgentAtRemoval<'a> {
    inner: &'a GitPlatform,
    host: &'a FakeHost,
    place: Place,
    cwd: PathBuf,
    commit: OnceLock<String>,
}

impl Platform for AgentAtRemoval<'_> {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn process_started(&self, pid: Pid) -> Option<Timestamp> {
        self.inner.process_started(pid)
    }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        let removal = cmd.args.windows(2).any(|w| w == ["worktree", "remove"]);
        if removal && self.host.state(&self.place, Duration::ZERO).is_ok() {
            self.commit
                .set(commit(&self.cwd, "a commit as the worktree goes"))
                .ok();
        }
        self.inner.run(cmd, timeout)
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
        self.inner.write_atomic(path, bytes)
    }
    fn lock_file(
        &self,
        path: &Path,
        wait: Duration,
    ) -> Result<teamwire_core::FileLock, PlatformError> {
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

#[cfg(test)]
fn detail<'r>(report: &'r CloseReport, step: &str) -> &'r str {
    &report.steps.iter().find(|s| s.step == step).unwrap().detail
}

#[test]
fn a_detached_members_place_is_closed_in_remove_worktrees_before_its_worktree_goes() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    let api = rig.member("api");
    let place = rig.place_of("api");
    let host = rig.host.clone();
    let open_at_close_places = std::cell::Cell::new(None);
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "close_places" {
                open_at_close_places.set(Some(host.state(&place, Duration::ZERO).is_ok()));
            }
        },
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(
        open_at_close_places.get(),
        Some(false),
        "the place was already closed when close_places began"
    );
    // The notes follow the order of the work: the place, then the worktree.
    let removed = detail(&report, "remove_worktrees");
    let closed_at = removed.find(&format!("api's place {} closed", place.id));
    let gone_at = removed.find(&format!("api's worktree {} removed", api.cwd.display()));
    assert!(
        closed_at.is_some() && gone_at.is_some() && closed_at < gone_at,
        "{removed}"
    );
    assert!(!api.cwd.exists());
    assert_eq!(
        detail(&report, "close_places"),
        format!("api's place {} was already gone", place.id)
    );
}

#[test]
fn a_process_that_outlives_the_closed_place_keeps_the_worktree_until_it_ends() {
    let mut rig = Rig::new();
    let sha = detached_commit(&mut rig, "api");
    let api = rig.member("api");
    let place = rig.place_of("api");
    // A git command the hangup reached, still finishing after the place closed.
    let late = Pid::new(7001);
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![late],
        },
    );
    rig.platform.inner.set_alive(late, true);
    // The process never ends here, so the wait would always run out.
    rig.config.close_exit_wait_ms = 100;
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("remove_worktrees"),
        "{report:?}"
    );
    let why = detail(&report, "remove_worktrees");
    assert!(
        why.contains("7001") && why.contains("run close again"),
        "{why}"
    );
    assert!(
        rig.host.state(&place, Duration::ZERO).is_err(),
        "the place is closed"
    );
    assert!(
        api.cwd.is_dir(),
        "the worktree stays while the process runs"
    );

    // The process ends: a rerun removes the worktree and keeps the commit.
    rig.platform.inner.set_alive(late, false);
    let rerun = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(rerun.stopped_at, None, "{rerun:?}");
    assert!(!api.cwd.exists());
    assert_ne!(git(&rig.repo, &["for-each-ref", "--contains", &sha]), "");
}

#[test]
fn a_member_on_a_branch_keeps_its_place_until_close_places() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    rig.add_worktree_member("web", MemberKind::Implementer);
    let api = rig.place_of("api");
    let web = rig.place_of("web");
    let host = rig.host.clone();
    let open = std::cell::RefCell::new(Vec::new());
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            let web_open = host.state(&web, Duration::ZERO).is_ok();
            open.borrow_mut().push((step.to_owned(), web_open));
        },
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    let open_at = |step: &str| {
        open.borrow()
            .iter()
            .find(|(s, _)| s == step)
            .map(|(_, open)| *open)
    };
    assert_eq!(open_at("remove_worktrees"), Some(true));
    assert_eq!(open_at("close_places"), Some(true));
    assert_eq!(open_at("release_hold"), Some(false));
    assert_eq!(
        detail(&report, "close_places"),
        format!(
            "api's place {} was already gone; web's place {} closed",
            api.id, web.id
        )
    );
}

#[test]
fn every_commit_the_agent_makes_during_remove_worktrees_is_kept_on_a_ref() {
    let mut rig = Rig::new();
    let first = detached_commit(&mut rig, "api");
    let cwd = rig.member("api").cwd;
    let (platform, host) = (rig.platform.clone(), rig.host.clone());
    let agent = AgentAtRemoval {
        inner: &platform,
        host: &host,
        place: rig.place_of("api"),
        cwd: cwd.clone(),
        commit: OnceLock::new(),
    };
    let last = std::cell::RefCell::new(String::new());
    let mut ctx = Ctx {
        home: rig.home.path(),
        store: &mut rig.store,
        registry: &rig.registry,
        platform: &agent,
        clock: &rig.clock,
        config: &rig.config,
    };
    // The agent's commit as the step begins, before the rescue branch.
    let report = close_observed(&mut ctx, CloseTarget::Team, CloseOpts::default(), &|step| {
        if step == "remove_worktrees" {
            *last.borrow_mut() = commit(&cwd, "a last commit");
        }
    });
    let last = last.into_inner();
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!cwd.exists());
    let rescue = format!(
        "teamwire/rescue/{}-api-{}",
        rig::TEAM,
        last.get(..7).unwrap()
    );
    assert_eq!(report.branches_kept, ["br-api".to_owned(), rescue.clone()]);
    assert_eq!(git(&rig.repo, &["rev-parse", &rescue]), last);
    let at_removal = agent.commit.get();
    for sha in [&first, &last].into_iter().chain(at_removal) {
        assert_ne!(
            git(&rig.repo, &["for-each-ref", "--contains", sha]),
            "",
            "{sha} is on no ref"
        );
    }
    assert_eq!(
        at_removal, None,
        "the place was closed before git worktree remove"
    );
}

#[test]
fn the_dry_run_names_each_detached_member_whose_place_closes_first() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    let web = rig.add_worktree_member("web", MemberKind::Implementer);
    let api = rig.member("api");
    let plan = teamwire_close::plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{plan:?}");
    let step = plan.steps.iter().find(|s| s.step == "remove_worktrees");
    assert_eq!(
        step.map(|s| s.detail.clone()),
        Some(format!(
            "git worktree remove, without force: api ({}), web ({}); api's HEAD is detached, so \
             its place is closed right before its worktree is removed",
            api.cwd.display(),
            web.cwd.display()
        )),
        "{plan:?}"
    );
}

#[test]
fn the_worktree_is_still_there_when_the_place_closes_and_goes_once_its_processes_end() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    let cwd = rig.member("api").cwd;
    let place = rig.place_of("api");
    // A git command the hangup reached, still finishing as the place closes.
    let late = Pid::new(7003);
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![late],
        },
    );
    rig.platform.inner.set_alive(late, true);
    let (host, platform) = (rig.host.clone(), rig.platform.clone());
    let finished = std::sync::atomic::AtomicBool::new(false);
    let (report, seen) = std::thread::scope(|s| {
        // The moment the place is gone, look at the worktree, then let the
        // process end. Gives up with the close, or after the suite's 60 s.
        let watcher = s.spawn(|| {
            for _ in 0..12_000 {
                if finished.load(std::sync::atomic::Ordering::SeqCst) {
                    return None;
                }
                if host.state(&place, Duration::ZERO).is_err() {
                    let there = cwd.is_dir();
                    platform.inner.set_alive(late, false);
                    return Some(there);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            None
        });
        let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
        finished.store(true, std::sync::atomic::Ordering::SeqCst);
        (report, watcher.join().unwrap())
    });
    assert_eq!(
        seen,
        Some(true),
        "the worktree was there when the place closed"
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!cwd.exists(), "removed once the process ended");
}
