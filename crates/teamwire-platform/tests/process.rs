//! `pid_alive` and `env_of`, against children the test starts itself.

#![expect(
    clippy::disallowed_methods,
    reason = "these tests need long-lived children that Platform::run would wait for"
)]
#![expect(
    clippy::unwrap_used,
    reason = "unwrap is allowed in #[test] functions only; the helpers here are test code too"
)]

use std::process::{Child, Command, Stdio};

use teamwire_core::{Pid, PlatformErrorCode};
use teamwire_platform::native;

/// Set in a child to make `sleeper_entry` sleep instead of returning.
const SLEEP_VAR: &str = "TW_T03_SLEEP";

/// Not a real test. When a child runs this test binary with `SLEEP_VAR` set,
/// this is the process the test reads and signals; otherwise it returns at
/// once. macOS hides the environment of Apple-signed binaries such as
/// `/bin/sleep`, and a copy of one is killed by the kernel within
/// milliseconds, so the probe is this binary, which the linker signed.
#[test]
fn sleeper_entry() {
    if std::env::var_os(SLEEP_VAR).is_some() {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Kills and reaps its child on every path, including a failed assertion.
struct Sleeper(Child);

impl Sleeper {
    fn start(setup: impl FnOnce(&mut Command)) -> Self {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", "sleeper_entry", "--test-threads=1"])
            .env(SLEEP_VAR, "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        setup(&mut cmd);
        Self(cmd.spawn().unwrap())
    }

    fn pid(&self) -> Pid {
        Pid::new(self.0.id())
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

#[test]
fn pid_alive_follows_the_life_of_a_child() {
    let mut child = Sleeper::start(|_| {});
    let pid = child.pid();
    assert!(native().pid_alive(pid));
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(!native().pid_alive(pid));
}

#[test]
fn pid_alive_is_false_for_pid_zero_and_out_of_range_pids() {
    assert!(!native().pid_alive(Pid::new(0)));
    assert!(!native().pid_alive(Pid::new(u32::MAX)));
}

#[test]
fn pid_alive_is_true_for_this_process() {
    assert!(native().pid_alive(Pid::new(std::process::id())));
}

#[test]
fn env_of_reads_a_variable_of_a_same_user_child() {
    let child = Sleeper::start(|cmd| {
        cmd.env("TW_T03_ENV", "hello=world");
    });
    let got = native().env_of(child.pid(), "TW_T03_ENV").unwrap();
    assert_eq!(got.as_deref(), Some("hello=world"));
}

#[test]
fn env_of_reports_an_unset_variable_as_none() {
    let child = Sleeper::start(|cmd| {
        cmd.env("TW_T03_ENV", "x");
    });
    assert_eq!(
        native().env_of(child.pid(), "TW_T03_NOT_SET").unwrap(),
        None
    );
    // A prefix of a real name is not that name.
    assert_eq!(native().env_of(child.pid(), "TW_T03").unwrap(), None);
}

#[test]
fn env_of_a_cleared_environment_is_none() {
    let child = Sleeper::start(|cmd| {
        cmd.env_clear().env(SLEEP_VAR, "1");
    });
    assert_eq!(native().env_of(child.pid(), "HOME").unwrap(), None);
}

/// The kernel hides the environment of Apple-signed binaries; that must be an
/// error, never "variable not set".
#[cfg(target_os = "macos")]
#[test]
fn env_of_a_system_binary_is_an_error_not_none() {
    let mut child = Command::new("/bin/sleep")
        .arg("60")
        .env("TW_T03_ENV", "x")
        .spawn()
        .unwrap();
    let got = native().env_of(Pid::new(child.id()), "TW_T03_ENV");
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(got.unwrap_err().code, PlatformErrorCode::PermissionDenied);
}

#[test]
fn env_of_a_gone_process_is_not_found() {
    let mut child = Sleeper::start(|_| {});
    let pid = child.pid();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let err = native().env_of(pid, "HOME").unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::NotFound);
}

#[test]
fn the_probe_is_still_alive_after_two_seconds() {
    let child = Sleeper::start(|_| {});
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(native().pid_alive(child.pid()));
    assert_eq!(
        native().env_of(child.pid(), SLEEP_VAR).unwrap().as_deref(),
        Some("1")
    );
}
