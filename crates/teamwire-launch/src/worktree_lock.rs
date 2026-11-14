//! The worktree lock: the kernel's `flock` on
//! `$TEAMWIRE_HOME/locks/worktree-<sha256 of the git common directory>.lock`,
//! held around each `git worktree` call that changes a repository's
//! `.git/worktrees/`. git takes no lock there itself: an add reads every
//! entry in that folder, so it can read another add's half-made entry and
//! fail with `failed to read .git/worktrees/<name>/commondir`. Close takes
//! the same lock around its removes and prunes.
//!
//! The key is the git common directory, so a launch into a linked worktree
//! and one into the main checkout wait for each other. `Platform::lock_file`
//! takes the lock: it ends with the open file, so a holder that is killed
//! leaves nothing held, and the file is made if missing and never deleted.
//! It is held around one git call only, never across setup, a host call or
//! a store write, and never together with the exclude lock, so the two locks
//! cannot deadlock.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use teamwire_core::{FileLock, Platform, PlatformErrorCode};

use crate::git::stdout_of;

/// How long launch waits for another process's worktree lock: 300 s. Each
/// holder runs one git call, which the git timeout ends after 120 s, so the
/// wait covers one slow holder and others queued ahead of this one.
///
/// shortcut: each locked git call waits the full 300 s on its own and
/// prints nothing meanwhile, so a stopped (Ctrl-Z) or hung holder costs a
/// launch 300 s for every worktree member; share one deadline across a
/// run's calls and say who holds the lock if a stuck holder is ever seen.
const LOCK_WAIT: Duration = Duration::from_secs(300);

/// Takes the worktree lock of the repository `repo` belongs to. Dropping
/// the guard closes the file, which releases the lock.
///
/// shortcut: the lock ends with this process, not with the git call it
/// covers. git runs in a process group of its own, so when teamwire is
/// killed mid-call the lock is freed while git still writes
/// `.git/worktrees/`, and a second launch can race it; let the git child
/// inherit the locked file, or kill its group when teamwire dies, if a
/// launch is ever seen failing right after another was killed.
pub(crate) fn lock_worktrees(
    platform: &dyn Platform,
    home: &Path,
    repo: &Path,
) -> Result<FileLock, String> {
    let common = common_dir(platform, repo)?;
    let digest = Sha256::digest(common.to_string_lossy().as_bytes());
    let path = home
        .join("locks")
        .join(format!("worktree-{}.lock", hex::encode(digest)));
    platform.lock_file(&path, LOCK_WAIT).map_err(|e| {
        if e.code == PlatformErrorCode::TimedOut {
            format!(
                "another teamwire process held the worktree lock {} on {} for {} s; try again",
                path.display(),
                common.display(),
                LOCK_WAIT.as_secs()
            )
        } else {
            e.detail
        }
    })
}

/// The repository's git common directory, absolute, with symlinks resolved,
/// so one repository reached two ways is one key.
fn common_dir(platform: &dyn Platform, repo: &Path) -> Result<PathBuf, String> {
    let text = stdout_of(platform, repo, &["rev-parse", "--git-common-dir"])?;
    let path = PathBuf::from(text);
    let path = if path.is_absolute() {
        path
    } else {
        repo.join(path)
    };
    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}
