//! The worktree lock close holds around `git worktree remove` and `git
//! worktree prune`: the kernel's `flock` on
//! `$TEAMWIRE_HOME/locks/worktree-<sha256 of the git common
//! directory>.lock`, the lock launch holds around `git worktree add`. A
//! remove or a prune deletes entries of `.git/worktrees/`, which an add
//! running at the same time reads.

#[path = "support/rig.rs"]
pub mod rig;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::ScopedJoinHandle;
use std::time::Duration;

use rig::Rig;
use sha2::{Digest, Sha256};
use teamwire_close::{CloseOpts, CloseTarget, close_observed};
use teamwire_core::{MemberKind, Role};

const PAUSE: Duration = Duration::from_millis(50);

/// How long a test watches a step that a held lock must hold back. git
/// removes or prunes a worktree of the test repository in well under this.
const WATCH: Duration = Duration::from_secs(1);

/// The worktree lock file launch and close take for the rig's repository.
#[cfg(test)]
fn lock_path(rig: &Rig) -> PathBuf {
    let common = std::fs::canonicalize(rig.repo.join(".git")).unwrap();
    let digest = Sha256::digest(common.to_string_lossy().as_bytes());
    rig.home
        .path()
        .join("locks")
        .join(format!("worktree-{}.lock", hex::encode(digest)))
}

/// Holds the lock through an open file of the test's own, as another
/// teamwire process would. Dropping the file releases the lock.
#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the test holds the lock through a file of its own"
)]
fn hold(path: &Path) -> File {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap();
    file.try_lock().unwrap();
    file
}

/// True once `flag` is set. Waits up to 60 s, the suite's budget, and gives
/// up early when `task` has finished.
fn reached<T>(flag: &AtomicBool, task: &ScopedJoinHandle<'_, T>) -> bool {
    for _ in 0..1200 {
        if flag.load(Ordering::SeqCst) || task.is_finished() {
            break;
        }
        std::thread::sleep(PAUSE);
    }
    flag.load(Ordering::SeqCst)
}

/// True when, for all of `WATCH`, `path` still exists and `task` is still
/// running: the step that would delete `path` is waiting.
fn held_back<T>(path: &Path, task: &ScopedJoinHandle<'_, T>) -> bool {
    let mut watched = Duration::ZERO;
    while watched < WATCH {
        if !path.exists() || task.is_finished() {
            return false;
        }
        std::thread::sleep(PAUSE);
        watched += PAUSE;
    }
    path.exists() && !task.is_finished()
}

/// Closes `target` while the test holds the worktree lock, and checks that
/// `path` stays until the lock is released, then goes.
#[cfg(test)]
fn closes_only_once_the_lock_is_free(mut rig: Rig, target: CloseTarget, path: &Path) {
    let lock = lock_path(&rig);
    let held = hold(&lock);
    let at_step = AtomicBool::new(false);
    std::thread::scope(|s| {
        let closing = s.spawn(|| {
            // Team close names the step `remove_worktrees`, a removal
            // `remove_worktree`.
            let observer = |step: &str| {
                if step.starts_with("remove_worktree") {
                    at_step.store(true, Ordering::SeqCst);
                }
            };
            close_observed(&mut rig.ctx(), target, CloseOpts::default(), &observer)
        });
        assert!(
            reached(&at_step, &closing),
            "close reached the worktree step"
        );
        assert!(
            held_back(path, &closing),
            "{} was deleted, or the close ended, while another process held the lock",
            path.display()
        );
        drop(held);
        let report = closing.join().unwrap();
        assert_eq!(report.stopped_at, None, "{report:?}");
        assert!(report.refusals.is_empty(), "{report:?}");
    });
    assert!(!path.exists(), "{} is gone after the close", path.display());
    assert!(
        File::open(&lock).unwrap().try_lock().is_ok(),
        "the close released the lock"
    );
}

#[test]
fn a_close_removes_no_worktree_while_another_process_holds_the_worktree_lock() {
    let mut rig = Rig::new();
    let worktree = rig.add_worktree_member("api", MemberKind::Implementer).cwd;
    closes_only_once_the_lock_is_free(rig, CloseTarget::Team, &worktree);
}

#[test]
fn a_close_prunes_no_worktree_entry_while_another_process_holds_the_worktree_lock() {
    let mut rig = Rig::new();
    let worktree = rig.add_worktree_member("api", MemberKind::Implementer).cwd;
    // A worktree whose folder is gone leaves only its entry, which the
    // prune deletes.
    std::fs::remove_dir_all(&worktree).unwrap();
    let entry = rig.repo.join(".git/worktrees/wt-api");
    assert!(entry.is_dir(), "git still lists the worktree");
    closes_only_once_the_lock_is_free(rig, CloseTarget::Team, &entry);
}

#[test]
fn a_member_removal_removes_no_worktree_while_another_process_holds_the_worktree_lock() {
    let mut rig = Rig::new();
    let worktree = rig.add_worktree_member("rev", MemberKind::Reviewer).cwd;
    let target = CloseTarget::Member(Role::parse("rev").unwrap());
    closes_only_once_the_lock_is_free(rig, target, &worktree);
}

#[test]
fn a_member_removal_prunes_no_worktree_entry_while_another_process_holds_the_worktree_lock() {
    let mut rig = Rig::new();
    let worktree = rig.add_worktree_member("rev", MemberKind::Reviewer).cwd;
    std::fs::remove_dir_all(&worktree).unwrap();
    let entry = rig.repo.join(".git/worktrees/wt-rev");
    assert!(entry.is_dir(), "git still lists the worktree");
    let target = CloseTarget::Member(Role::parse("rev").unwrap());
    closes_only_once_the_lock_is_free(rig, target, &entry);
}
