//! Two `teamwire user-key` runs at once on a home with no key (ADR 0040):
//! the second run keeps the key the first one wrote, so a dashboard that
//! read the key as soon as its run ended still holds the key in the file.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use teamwire_core::platform::{Command, RunOutput, SleepHold};
use teamwire_core::{Pid, Platform, PlatformError};
use teamwire_fakes::{FakePlatform, TestHome};

/// The fake platform, except that the first write pauses before it starts,
/// as a slow disk would. A second run then starts while the first one sits
/// between finding no key and writing its own.
#[derive(Debug)]
struct FirstWritePauses {
    inner: FakePlatform,
    paused: AtomicBool,
}

impl Platform for FirstWritePauses {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        self.inner.run(cmd, timeout)
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
        if !self.paused.swap(true, Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(300));
        }
        self.inner.write_atomic(path, bytes)
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

#[test]
fn a_second_run_during_the_first_keeps_the_first_runs_key() {
    let home = TestHome::new().unwrap();
    let platform = FirstWritePauses {
        inner: FakePlatform::new(),
        paused: AtomicBool::new(false),
    };
    // The key each run's caller reads as soon as its run returns.
    let read: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let run = |wait: Duration| {
        std::thread::sleep(wait);
        let path = teamwire_launch::ensure_user_key(&platform, home.path()).unwrap();
        read.lock()
            .unwrap()
            .push(std::fs::read_to_string(path).unwrap());
    };
    std::thread::scope(|s| {
        s.spawn(|| run(Duration::ZERO));
        s.spawn(|| run(Duration::from_millis(100)));
    });
    let last = std::fs::read_to_string(home.path().join("user.key")).unwrap();
    assert_eq!(last.len(), 65, "{last:?}");
    let read = read.into_inner().unwrap();
    assert_eq!(read, vec![last.clone(), last]);
}
