//! Operating-system facts behind the `Platform` and `Clock` traits: pid
//! liveness, a process's environment, running a command in its own process
//! group with a timeout, atomic file writes, free space, and the sleep hold
//! (`caffeinate` on macOS).
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

fn not_built() -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::Other,
        "teamwire-platform is not built yet",
    )
}

/// macOS: every call, including the `caffeinate` sleep hold.
#[derive(Debug, Clone, Copy, Default)]
pub struct MacOs;

/// Linux: liveness and environment; no sleep hold.
#[derive(Debug, Clone, Copy, Default)]
pub struct Linux;

macro_rules! not_built_platform {
    ($name:ident) => {
        impl Platform for $name {
            fn pid_alive(&self, _pid: Pid) -> bool {
                false
            }

            fn env_of(&self, _pid: Pid, _var: &str) -> Result<Option<String>, PlatformError> {
                Err(not_built())
            }

            fn run(&self, _cmd: &Command, _timeout: Duration) -> Result<RunOutput, PlatformError> {
                Err(not_built())
            }

            fn write_atomic(&self, _path: &Path, _bytes: &[u8]) -> Result<(), PlatformError> {
                Err(not_built())
            }

            fn sleep_hold(&self) -> Option<&dyn SleepHold> {
                None
            }

            fn free_bytes(&self, _path: &Path) -> Result<u64, PlatformError> {
                Err(not_built())
            }

            fn on_network_fs(&self, _path: &Path) -> Result<bool, PlatformError> {
                Err(not_built())
            }
        }
    };
}

not_built_platform!(MacOs);
not_built_platform!(Linux);

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
