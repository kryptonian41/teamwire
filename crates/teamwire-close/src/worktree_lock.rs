//! The worktree lock launch also takes (teamwire-launch's
//! `worktree_lock.rs`): the kernel's `flock` on
//! `$TEAMWIRE_HOME/locks/worktree-<sha256 of the git common directory>.lock`,
//! held around each `git worktree remove` and `git worktree prune`. Both
//! delete entries of `.git/worktrees/`, which a launch's `git worktree add`
//! running at the same time reads, and git takes no lock there itself.
//!
//! The lock ends with the open file, so a holder that is killed leaves
//! nothing held; the file is made if missing and never deleted. Close holds
//! it around one git call only, never during a store write, and never
//! together with the exclude lock, so the two locks cannot deadlock.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use teamwire_core::Platform;

use crate::git;

/// How often, and how long apart, close tries for another process's
/// worktree lock: 300 s. Each holder runs one git call, which the git
/// timeout ends after 120 s, so the wait covers one slow holder and
/// others queued ahead of this one.
const LOCK_TRIES: u32 = 6000;
const LOCK_PAUSE: Duration = Duration::from_millis(50);

/// A held worktree lock. Dropping it closes the file, which releases the
/// lock.
pub(crate) struct WorktreeLock {
    _file: File,
}

/// Takes the worktree lock of the repository `repo` belongs to.
#[expect(
    clippy::disallowed_methods,
    reason = "the worktree lock file is only ever locked, never written"
)]
pub(crate) fn lock_worktrees(
    platform: &dyn Platform,
    home: &Path,
    repo: &Path,
) -> Result<WorktreeLock, String> {
    let common = common_dir(platform, repo)?;
    let locks = home.join("locks");
    std::fs::create_dir_all(&locks).map_err(|e| format!("cannot make {}: {e}", locks.display()))?;
    let digest = Sha256::digest(common.to_string_lossy().as_bytes());
    let path = locks.join(format!("worktree-{}.lock", hex::encode(digest)));
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("cannot open the lock {}: {e}", path.display()))?;
    for _ in 0..LOCK_TRIES {
        match lock.try_lock() {
            Ok(()) => return Ok(WorktreeLock { _file: lock }),
            Err(TryLockError::WouldBlock) => std::thread::sleep(LOCK_PAUSE),
            Err(TryLockError::Error(e)) => {
                return Err(format!("cannot take the lock {}: {e}", path.display()));
            }
        }
    }
    Err(format!(
        "another teamwire process held the worktree lock {} on {} for {} s",
        path.display(),
        common.display(),
        (LOCK_PAUSE * LOCK_TRIES).as_secs()
    ))
}

/// `git worktree prune`, never with force, under the worktree lock. A prune
/// that cannot run is skipped, as before the lock: the entries it would
/// delete name folders that are gone, and the next prune deletes them.
pub(crate) fn prune(platform: &dyn Platform, home: &Path, repo: &Path) {
    if let Ok(_lock) = lock_worktrees(platform, home, repo) {
        git::run(platform, repo, &["worktree", "prune"]).ok();
    }
}

/// The repository's git common directory, absolute, with symlinks resolved,
/// so one repository reached two ways is one key.
fn common_dir(platform: &dyn Platform, repo: &Path) -> Result<PathBuf, String> {
    let text = git::stdout_of(platform, repo, &["rev-parse", "--git-common-dir"])?;
    let path = PathBuf::from(text);
    let path = if path.is_absolute() {
        path
    } else {
        repo.join(path)
    };
    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}
