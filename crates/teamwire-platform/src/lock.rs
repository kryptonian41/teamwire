//! `Platform::lock_file`: the kernel's `flock` on a lock file, the one
//! implementation behind launch's and close's exclude, worktree and
//! user-key locks. The lock ends with the open file, so a holder that is
//! killed holds nothing, and nothing ever deletes a lock file.
//!
//! The error details are the texts launch and close show the user, which
//! is why they read "cannot open the lock ..." rather than this crate's
//! usual "opening ...". Only a timeout is left for the caller to word,
//! because only the caller knows what the lock guards.

use std::fs::{OpenOptions, TryLockError};
use std::path::Path;
use std::thread;
use std::time::Duration;

use teamwire_core::{Clock, FileLock, PlatformError, PlatformErrorCode};

use crate::{SystemClock, io_error};

/// How long apart the tries for a busy lock are: short next to any wait a
/// caller sets, long enough not to burn a core while a holder works.
const PAUSE: Duration = Duration::from_millis(50);

/// Makes the lock file and its folder when missing, then takes the lock,
/// trying again every `PAUSE` until `wait` has passed.
pub(crate) fn lock_file(path: &Path, wait: Duration) -> Result<FileLock, PlatformError> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| io_error(&format!("cannot make {}", dir.display()), &e))?;
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "a lock file is only ever locked, never written"
    )]
    let opened = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path);
    let file =
        opened.map_err(|e| io_error(&format!("cannot open the lock {}", path.display()), &e))?;
    let clock = SystemClock;
    // A wait too large for `Instant` means "as long as the holder holds":
    // adding it would panic.
    let deadline = clock.monotonic().checked_add(wait);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(FileLock::new(file)),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => {
                return Err(io_error(
                    &format!("cannot take the lock {}", path.display()),
                    &e,
                ));
            }
        }
        let pause = match deadline {
            None => PAUSE,
            Some(deadline) => {
                let now = clock.monotonic();
                if now >= deadline {
                    return Err(PlatformError::new(
                        PlatformErrorCode::TimedOut,
                        format!(
                            "the lock {} stayed busy for {} ms",
                            path.display(),
                            wait.as_millis()
                        ),
                    ));
                }
                PAUSE.min(deadline - now)
            }
        };
        thread::sleep(pause);
    }
}
