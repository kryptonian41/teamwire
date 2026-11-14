//! The worktree lock: launch holds the kernel's `flock` on
//! `$TEAMWIRE_HOME/locks/worktree-<sha256 of the git common
//! directory>.lock` around each `git worktree` call that changes the
//! repository's `.git/worktrees/`. git takes no such lock itself, so two
//! adds into one repository at once can read each other's half-made entry
//! and fail with `failed to read .git/worktrees/<name>/commondir`.

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use std::path::{Path, PathBuf};
use std::thread::ScopedJoinHandle;
use std::time::Duration;

use common::{Rig, git, one_worktree_team};
use teamwire_launch::LaunchOutcome;

const PAUSE: Duration = Duration::from_millis(50);

/// How long a test watches a step that a held lock must hold back. git
/// adds or removes a worktree of the test repository in well under this.
const WATCH: Duration = Duration::from_secs(1);

/// The worktree lock file for the repository whose git common directory
/// (`<repo>/.git` for a main checkout) is `common`.
fn worktree_lock(rig: &Rig, common: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let key = std::fs::canonicalize(common).unwrap();
    let digest = Sha256::digest(key.to_string_lossy().as_bytes());
    rig.home
        .path()
        .join("locks")
        .join(format!("worktree-{}.lock", hex::encode(digest)))
}

/// Holds the lock at `path` through an open file of the test's own, as
/// another teamwire process would. Dropping the file releases the lock.
fn hold(path: &Path) -> std::fs::File {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    if !path.exists() {
        common::write(path, "");
    }
    let file = std::fs::File::open(path).unwrap();
    file.try_lock().unwrap();
    file
}

/// True once `path` exists. Waits up to 60 s, the suite's budget, and gives
/// up early when `task` has finished without making it.
fn appears<T>(path: &Path, task: &ScopedJoinHandle<'_, T>) -> bool {
    for _ in 0..1200 {
        if path.exists() || task.is_finished() {
            break;
        }
        std::thread::sleep(PAUSE);
    }
    path.exists()
}

/// True when, for all of `WATCH`, `path` exists exactly when `present` says
/// and `task` is still running: the step that would change `path` is
/// waiting.
fn held_back<T>(path: &Path, present: bool, task: &ScopedJoinHandle<'_, T>) -> bool {
    let mut watched = Duration::ZERO;
    while watched < WATCH {
        if path.exists() != present || task.is_finished() {
            return false;
        }
        std::thread::sleep(PAUSE);
        watched += PAUSE;
    }
    path.exists() == present && !task.is_finished()
}

#[test]
fn a_launch_adds_no_worktree_while_another_process_holds_the_worktree_lock() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let lock = worktree_lock(&rig, &repo.join(".git"));
    let held = hold(&lock);
    let worktree = repo.join(".worktrees/api");
    std::thread::scope(|s| {
        let launch = s.spawn(|| rig.launch("demo", rig.validate(&spec, false).unwrap()));
        // Launch makes the folder of its worktrees right before it asks
        // for the lock.
        assert!(
            appears(&repo.join(".worktrees"), &launch),
            "launch reached the worktree step"
        );
        assert!(
            held_back(&worktree, false, &launch),
            "git worktree add ran while another process held the lock"
        );
        drop(held);
        let (report, _) = launch.join().unwrap();
        assert_eq!(
            report.members.first().unwrap().outcome,
            LaunchOutcome::Started
        );
    });
    assert!(worktree.join("README.md").is_file(), "the worktree exists");
    assert!(
        std::fs::File::open(&lock).unwrap().try_lock().is_ok(),
        "the launch released the lock"
    );
}

#[test]
fn a_launch_into_a_linked_worktree_waits_for_the_lock_of_the_main_checkout() {
    let rig = Rig::new();
    let main = rig.repo("proj");
    let linked = rig.work.path().join("proj-linked");
    let shown = linked.display().to_string();
    git(&main, &["worktree", "add", "-q", "-b", "side", &shown]);
    let spec = common::team_spec(
        "demo",
        &[common::worktree_member("api", "API work", &linked)],
    );
    // One repository has one `.git/worktrees/`, whichever checkout names it.
    let held = hold(&worktree_lock(&rig, &main.join(".git")));
    let worktree = linked.join(".worktrees/api");
    std::thread::scope(|s| {
        let launch = s.spawn(|| rig.launch("demo", rig.validate(&spec, false).unwrap()));
        assert!(
            appears(&linked.join(".worktrees"), &launch),
            "launch reached the worktree step"
        );
        assert!(
            held_back(&worktree, false, &launch),
            "git worktree add ran while another process held the main checkout's lock"
        );
        drop(held);
        let (report, _) = launch.join().unwrap();
        assert_eq!(
            report.members.first().unwrap().outcome,
            LaunchOutcome::Started
        );
    });
    assert!(worktree.join("README.md").is_file(), "the worktree exists");
}

#[test]
fn a_failed_launch_removes_its_worktree_only_once_the_worktree_lock_is_free() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let started = rig.work.path().join("setup-started");
    let go = rig.work.path().join("setup-go");
    // Setup says it is running, then fails once the test says so. Both
    // marker files are outside the worktree, so its removal needs no force.
    let script = format!(
        "touch '{}'; while [ ! -e '{}' ]; do sleep 0.05; done; exit 1",
        started.display(),
        go.display()
    );
    let member = common::with(
        common::worktree_member("api", "API work", &repo),
        "setup",
        serde_json::json!(script),
    );
    let spec = common::team_spec("demo", &[member]);
    let lock = worktree_lock(&rig, &repo.join(".git"));
    let worktree = repo.join(".worktrees/api");
    std::thread::scope(|s| {
        let launch = s.spawn(|| rig.launch("demo", rig.validate(&spec, false).unwrap()));
        assert!(appears(&started, &launch), "setup ran");
        let held = hold(&lock);
        common::write(&go, "");
        assert!(
            held_back(&worktree, true, &launch),
            "git worktree remove ran while another process held the lock"
        );
        drop(held);
        let (report, _) = launch.join().unwrap();
        let outcome = &report.members.first().unwrap().outcome;
        assert!(matches!(outcome, LaunchOutcome::Failed(_)), "{outcome:?}");
    });
    assert!(!worktree.exists(), "the undo removed the worktree");
    assert!(
        std::fs::File::open(&lock).unwrap().try_lock().is_ok(),
        "the undo released the lock"
    );
}
