//! The shared platform contract suite, against the real macOS platform.
//!
//! The sleep hold here waits on a `sleep` child this test starts and kills,
//! so the suite can never find, reuse or release a hold the user or the lead
//! session runs. The environment probe reads this test's own pid, because
//! macOS hides the environment of Apple-signed binaries such as `sleep`.

#![cfg(target_os = "macos")]
#![expect(
    clippy::disallowed_methods,
    reason = "the suite needs a long-lived child and a reaped child that Platform::run would wait for"
)]

use std::process::{Child, Command};

use teamwire_core::Pid;
use teamwire_fakes::contract::{EnvProbe, PlatformSetup, RunCase, platform_contract};
use teamwire_platform::MacOs;

/// Kills the hold's lead child on every path, including a failed rule.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        // The child may already be gone; either way nothing is left running.
        let _killed = self.0.kill();
        let _reaped = self.0.wait();
    }
}

#[test]
fn macos_passes_the_platform_contract() {
    let dir = tempfile::tempdir().unwrap();
    let lead = KillOnDrop(Command::new("sleep").arg("60").spawn().unwrap());
    let mut dead = Command::new("true").spawn().unwrap();
    dead.wait().unwrap();
    let own = Pid::new(std::process::id());

    platform_contract(&PlatformSetup {
        platform: &MacOs,
        dir: dir.path(),
        live_pid: own,
        dead_pid: Some(Pid::new(dead.id())),
        env_probe: Some(EnvProbe {
            pid: own,
            var: "PATH".into(),
            value: std::env::var("PATH").unwrap(),
        }),
        run_cases: RunCase::unix_standard(),
        hold_lead: Pid::new(lead.0.id()),
    });
}
