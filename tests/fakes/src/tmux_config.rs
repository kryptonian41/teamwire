//! `NoTmuxConfig`: a real `Platform` whose every `tmux` call gets
//! `-f /dev/null`, so a test's private tmux server never reads the user's
//! `~/.tmux.conf`. That file can run plugin scripts when a server starts,
//! which take seconds on a busy machine and push a test's `spawn` past its
//! timeout; it also makes a test depend on how one user set up tmux. tmux
//! reads the file only when a server starts, so the flag matters only on the
//! call that starts one, and every other call is unchanged.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{Command, Pid, Platform, PlatformError, RunOutput, SleepHold};

/// Wraps a real platform; see the module note.
pub struct NoTmuxConfig {
    inner: Arc<dyn Platform>,
}

impl NoTmuxConfig {
    pub fn new(inner: Arc<dyn Platform>) -> Self {
        Self { inner }
    }
}

impl std::fmt::Debug for NoTmuxConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoTmuxConfig").finish_non_exhaustive()
    }
}

impl Platform for NoTmuxConfig {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }

    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }

    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        if cmd.program != "tmux" {
            return self.inner.run(cmd, timeout);
        }
        let mut cmd = cmd.clone();
        cmd.args
            .splice(0..0, ["-f".to_owned(), "/dev/null".to_owned()]);
        self.inner.run(&cmd, timeout)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
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
