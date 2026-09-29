//! The sleep hold, against processes the test starts itself.
//!
//! Every hold here waits on a `sleep 60` that this test file started, so no
//! test can find, reuse or kill a hold the user or the lead session runs.
//! `find_existing` matches only the `-w` pid it is given, and `release` is
//! only ever called on a hold `start` returned. The guards kill what a test
//! started on every path.

#![cfg(target_os = "macos")]
#![expect(
    clippy::disallowed_methods,
    reason = "these tests need long-lived children that Platform::run would wait for"
)]
#![expect(
    clippy::unwrap_used,
    reason = "unwrap is allowed in #[test] functions only; the helpers here are test code too"
)]

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use teamwire_core::{Clock, HoldInfo, Pid, Platform, ReleaseOutcome};
use teamwire_platform::{SystemClock, native};

/// A stand-in for the lead session. Killed and reaped on drop, and so is
/// every hold registered with it.
struct Lead {
    child: Child,
    holds: Vec<Pid>,
}

impl Lead {
    fn start() -> Self {
        let child = Command::new("/bin/sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self {
            child,
            holds: Vec::new(),
        }
    }

    fn pid(&self) -> Pid {
        Pid::new(self.child.id())
    }

    /// Start a hold on this lead through the platform and track it.
    fn hold(&mut self) -> HoldInfo {
        let hold = native().sleep_hold().unwrap().start(self.pid()).unwrap();
        self.holds.push(hold.pid);
        hold
    }
}

impl Drop for Lead {
    fn drop(&mut self) {
        // Only pids this test started: its own holds and its own child.
        for hold in &self.holds {
            if let Ok(raw) = i32::try_from(hold.get()) {
                kill(nix::unistd::Pid::from_raw(raw), Signal::SIGTERM).unwrap_or_default();
            }
        }
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let clock = SystemClock;
    let deadline = clock.monotonic() + Duration::from_secs(10);
    while !done() {
        if clock.monotonic() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

fn gone(pid: Pid) -> bool {
    wait_until(|| !native().pid_alive(pid))
}

#[test]
fn start_creates_an_owned_hold_that_find_existing_sees() {
    let mut lead = Lead::start();
    let platform = native();
    let holds = platform.sleep_hold().unwrap();
    assert_eq!(holds.find_existing(lead.pid()), None);

    let hold = lead.hold();
    assert!(hold.owned);
    assert!(platform.pid_alive(hold.pid));
    let found = holds.find_existing(lead.pid()).unwrap();
    assert_eq!(found.pid, hold.pid);
    // The process list cannot say who started it; the caller knows.
    assert!(!found.owned);
}

#[test]
fn a_hold_on_a_different_pid_is_ignored() {
    let mut lead_a = Lead::start();
    let lead_b = Lead::start();
    let hold_a = lead_a.hold();
    let platform = native();
    let holds = platform.sleep_hold().unwrap();
    assert_eq!(holds.find_existing(lead_a.pid()).unwrap().pid, hold_a.pid);
    assert_eq!(holds.find_existing(lead_b.pid()), None);
    // A pid no hold waits on, and one that a longer pid starts with.
    assert_eq!(holds.find_existing(Pid::new(u32::MAX)), None);
    let prefix_of = Pid::new(lead_a.pid().get() / 10);
    assert_eq!(holds.find_existing(prefix_of), None);
}

#[test]
fn release_only_last_owned() {
    let mut lead = Lead::start();
    let platform = native();
    let holds = platform.sleep_hold().unwrap();
    let hold = lead.hold();

    // A hold that teamwire did not start is left running.
    let user_hold = HoldInfo {
        pid: hold.pid,
        owned: false,
    };
    assert_eq!(
        holds.release(&user_hold, lead.pid()).unwrap(),
        ReleaseOutcome::NotOwned
    );
    assert!(native().pid_alive(hold.pid));

    // An owned hold for another session is not this lead's hold.
    let other = Pid::new(lead.pid().get() + 1);
    assert_eq!(
        holds.release(&hold, other).unwrap(),
        ReleaseOutcome::NotAHold
    );
    assert!(native().pid_alive(hold.pid));

    // The owned hold on this lead is released, once.
    assert_eq!(
        holds.release(&hold, lead.pid()).unwrap(),
        ReleaseOutcome::Released
    );
    // No waiting: `release` returns only once the hold is gone.
    assert!(!native().pid_alive(hold.pid));
    assert_eq!(holds.find_existing(lead.pid()), None);
    assert_eq!(
        holds.release(&hold, lead.pid()).unwrap(),
        ReleaseOutcome::AlreadyGone
    );
}

#[test]
fn a_pid_that_is_not_a_hold_is_never_killed() {
    let lead = Lead::start();
    let platform = native();
    let holds = platform.sleep_hold().unwrap();
    // The recorded hold pid now belongs to some other process, here the
    // lead's own `sleep`.
    let stale = HoldInfo {
        pid: lead.pid(),
        owned: true,
    };
    assert_eq!(
        holds.release(&stale, lead.pid()).unwrap(),
        ReleaseOutcome::NotAHold
    );
    assert!(native().pid_alive(lead.pid()));
}

#[test]
fn the_hold_ends_when_the_lead_ends() {
    let mut lead = Lead::start();
    let hold = lead.hold();
    drop(lead.child.kill());
    drop(lead.child.wait());
    assert!(gone(hold.pid), "caffeinate -w should exit with its pid");
}

#[test]
fn linux_has_no_sleep_hold() {
    assert!(teamwire_platform::Linux.sleep_hold().is_none());
}
