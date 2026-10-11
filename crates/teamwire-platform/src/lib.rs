//! Operating-system facts behind the `Platform` and `Clock` traits: pid
//! liveness, a process's start time and environment, running a command in
//! its own process group with a timeout, atomic file writes, free space, and
//! the sleep hold (`caffeinate` on macOS).
//!
//! Never: kill a sleep hold the user started; leave a timed-out process
//! group running. This is the only crate that spawns processes, reads the
//! system clock, or writes files outside `SQLite`.
//!
//! Invariants: `run` reports exit 124 when its timeout killed the group;
//! `write_atomic` leaves the old file or the new one, never a partial one.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use teamwire_core::{
    Clock, Command, Pid, Platform, PlatformError, PlatformErrorCode, RunOutput, SleepHold,
    Timestamp,
};
use time::OffsetDateTime;

mod files;
mod process;
mod run;
mod sleephold;

/// Map an I/O error to the platform error code a caller can act on.
pub(crate) fn io_error(what: &str, err: &std::io::Error) -> PlatformError {
    use std::io::ErrorKind;
    let kind = err.kind();
    let code = if kind == ErrorKind::NotFound {
        PlatformErrorCode::NotFound
    } else if kind == ErrorKind::PermissionDenied {
        PlatformErrorCode::PermissionDenied
    } else if kind == ErrorKind::StorageFull {
        PlatformErrorCode::NoSpace
    } else {
        PlatformErrorCode::Io
    };
    PlatformError::new(code, format!("{what}: {err}"))
}

/// macOS: every call, including the `caffeinate` sleep hold.
#[derive(Debug, Clone, Copy, Default)]
pub struct MacOs;

/// Linux: liveness, start time and environment; no sleep hold.
#[derive(Debug, Clone, Copy, Default)]
pub struct Linux;

macro_rules! platform_impl {
    ($name:ident, $me:ident => $hold:expr) => {
        impl Platform for $name {
            fn pid_alive(&self, pid: Pid) -> bool {
                process::pid_alive(pid)
            }

            fn process_started(&self, pid: Pid) -> Option<Timestamp> {
                process::process_started(pid)
            }

            fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
                process::env_of(pid, var)
            }

            fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
                run::run_command(cmd, timeout)
            }

            fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
                files::write_atomic(path, bytes)
            }

            fn sleep_hold(&self) -> Option<&dyn SleepHold> {
                let $me = self;
                $hold
            }

            fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError> {
                files::free_bytes(path)
            }

            fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError> {
                files::on_network_fs(path)
            }
        }
    };
}

platform_impl!(MacOs, me => Some(me));
platform_impl!(Linux, _me => None);

/// The platform this binary was built for.
pub fn native() -> Arc<dyn Platform> {
    if cfg!(target_os = "macos") {
        Arc::new(MacOs)
    } else {
        Arc::new(Linux)
    }
}

/// The real `Clock`: the only code allowed to read the system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_datetime(OffsetDateTime::now_utc())
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "SystemClock is the one Clock that reads the real monotonic clock"
    )]
    fn monotonic(&self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_moves_forward() {
        let clock = SystemClock;
        let a = clock.monotonic();
        let b = clock.monotonic();
        assert!(b >= a);
        assert!(clock.now().unix_millis() > 1_700_000_000_000);
    }
}
