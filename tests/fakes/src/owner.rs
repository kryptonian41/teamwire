//! Ties each `fake-claude` to the test process that made its world, so an
//! agent never outlives that process.
//!
//! `fake-herdr` exits as soon as an agent is ready, so the agent's parent is
//! launchd and nothing else ends it when the test process dies by a signal
//! (a runner's timeout, Ctrl-C). `FakeHerdrEnv`'s `Drop` cannot help then,
//! because it never runs.
//!
//! The test process holds an exclusive `flock` on a file for the world's
//! whole life. The kernel releases that lock when the process ends, however
//! it ends. An agent that can take a shared lock on the same file knows its
//! owner is gone and exits. A lock, unlike a pid, cannot be reused by an
//! unrelated process, and checking it starts no process.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::fsutil::Stopwatch;

/// The variable that names the lock file. It starts with `FAKE_`, so
/// `fake-herdr` passes it on to every agent it starts.
pub(crate) const OWNER_LOCK_VAR: &str = "FAKE_OWNER_LOCK";

/// How often an agent checks its owner. An agent ends at most this long
/// after its owner is gone.
const CHECK_EVERY: Duration = Duration::from_secs(1);

/// Creates the lock file and takes the owner's exclusive lock on it. The
/// lock lasts while the returned file stays open.
#[expect(
    clippy::disallowed_methods,
    reason = "the owner lock opens a file that is never replaced"
)]
pub(crate) fn hold(path: &Path) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    Ok(file)
}

/// An agent's view of its owner, read from `FAKE_OWNER_LOCK`. Without the
/// variable the owner never counts as gone, so a test that starts
/// `fake-claude` by hand keeps the old behaviour.
#[derive(Debug)]
pub(crate) struct Owner {
    lock: Option<PathBuf>,
    since_check: Stopwatch,
}

impl Owner {
    pub(crate) fn from_env() -> Self {
        Self {
            lock: std::env::var_os(OWNER_LOCK_VAR)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            since_check: Stopwatch::start(),
        }
    }

    /// True once the owner has ended. Looks at the lock at most once per
    /// `CHECK_EVERY`, so callers may ask on every pass of a fast loop.
    pub(crate) fn gone(&mut self) -> bool {
        if self.since_check.elapsed() < CHECK_EVERY {
            return false;
        }
        self.since_check = Stopwatch::start();
        let Some(path) = &self.lock else {
            return false;
        };
        // A missing file means the world's directory was removed, which
        // happens only after its owner let go.
        let Ok(file) = File::open(path) else {
            return true;
        };
        match file.try_lock_shared() {
            Ok(()) => true,
            Err(TryLockError::WouldBlock) => false,
            // A file system without flock: keep running, as before this check.
            Err(TryLockError::Error(_)) => false,
        }
    }

    /// Sleeps for `limit`, or less when the owner ends first. Returns true
    /// when the owner is gone.
    pub(crate) fn sleep(&mut self, limit: Duration) -> bool {
        let slept = Stopwatch::start();
        while slept.elapsed() < limit {
            if self.gone() {
                return true;
            }
            std::thread::sleep(CHECK_EVERY.min(limit.saturating_sub(slept.elapsed())));
        }
        false
    }
}
