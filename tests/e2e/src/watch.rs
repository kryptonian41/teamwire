//! A running `teamwire watch`, its output read from a file, killed on drop.

use std::path::PathBuf;
use std::process::Child;
use std::time::Duration;

use crate::world::wait_until;

/// One `teamwire watch` process. Dropping it kills the process, so a test
/// that fails half way leaves nothing running.
#[derive(Debug)]
pub struct Watch {
    pub(crate) child: Child,
    pub(crate) out: PathBuf,
}

impl Watch {
    /// Everything the watch printed on standard output so far.
    pub fn output(&self) -> String {
        std::fs::read_to_string(&self.out).unwrap_or_default()
    }

    /// The lines printed so far.
    pub fn lines(&self) -> Vec<String> {
        self.output().lines().map(str::to_owned).collect()
    }

    /// Waits until the output holds `needle`; panics with the output when
    /// `within` runs out.
    pub fn wait_for(&self, needle: &str, within: Duration) {
        let seen = wait_until(within, || self.output().contains(needle));
        assert!(
            seen,
            "the watch never printed {needle:?}: {}",
            self.output()
        );
    }

    /// Waits for the process to exit and returns its exit code.
    pub fn exit_code(&mut self, within: Duration) -> Option<i32> {
        let mut code = None;
        let done = wait_until(within, || match self.child.try_wait() {
            Ok(Some(status)) => {
                code = status.code();
                true
            }
            _ => false,
        });
        assert!(done, "the watch did not exit: {}", self.output());
        code
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // A watch that already exited makes both calls fail; that is fine.
        self.child.kill().ok();
        self.child.wait().ok();
    }
}
