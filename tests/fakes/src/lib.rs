//! Test doubles for every adapter trait (`FakeHost`, `FakeHarness`,
//! `FakePlatform`, `FakeClock`), the fake `herdr` and `claude` binaries with
//! the fixtures that run them, and the contract suites every adapter must
//! pass. `README.md` in this directory is the guide.
//!
//! Never: touch the real `~/.claude/`, `~/.claude.json` or `~/.teamwire`;
//! start a real herdr or Claude session; set process-wide environment.
//!
//! Invariants: every test's `TEAMWIRE_HOME` is a fresh temporary directory,
//! removed when the `TestHome` is dropped.

mod bins;
mod calls;
pub mod claude_bin;
mod clock;
pub mod contract;
mod fixture;
mod fsutil;
mod harness;
pub mod herdr_bin;
mod host;
mod platform;

pub use bins::{FakeBinDir, FakeBins};
pub use calls::{Call, CallKind, CallLog};
pub use claude_bin::{Fault as ClaudeFault, Script as ClaudeScript, Startup, Step};
pub use clock::FakeClock;
pub use fixture::{CliOutput, FakeHerdrEnv, HerdrFaults, LoggedCall};
pub use harness::{
    ESCALATED_TRUST_TEXT, FakeHarness, PLAIN_TRUST_TEXT, PROMPT_TEXT, TRUST_ESCALATION_MARKERS,
    TRUST_MARKERS, TRUST_OPTION_TEXT,
};
pub use host::{FakeHost, Fault, SpawnedWith};
pub use platform::{FakePlatform, FakeSleepHold, HoldEvent};

use std::path::Path;

/// A fresh, empty `TEAMWIRE_HOME` for one test, removed on drop. It never
/// sets the variable: callers pass `path()` to what they build, or put
/// `env()` into a child process's environment.
#[derive(Debug)]
pub struct TestHome {
    dir: tempfile::TempDir,
}

impl TestHome {
    pub fn new() -> std::io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("teamwire-home-")
            .tempdir()?;
        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The variable and value for a child process's environment.
    pub fn env(&self) -> (&'static str, &Path) {
        ("TEAMWIRE_HOME", self.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_home_is_fresh_empty_and_removed_on_drop() {
        let a = TestHome::new().unwrap();
        let b = TestHome::new().unwrap();
        assert_ne!(a.path(), b.path());
        assert!(a.path().is_dir());
        assert_eq!(std::fs::read_dir(a.path()).unwrap().count(), 0);
        assert_eq!(a.env(), ("TEAMWIRE_HOME", a.path()));
        let kept = a.path().to_path_buf();
        drop(a);
        assert!(!kept.exists());
    }
}
