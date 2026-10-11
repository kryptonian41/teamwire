//! A `Platform` that runs every command inside a private world: the real
//! platform, with the fake programs' environment on top of a cleared one.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{Command, Pid, Platform, PlatformError, RunOutput, SleepHold, Timestamp};

pub(crate) struct EnvPlatform {
    inner: Arc<dyn Platform>,
    env: Vec<(String, String)>,
}

impl EnvPlatform {
    pub(crate) fn new(env: Vec<(String, String)>) -> Self {
        Self {
            inner: teamwire_platform::native(),
            env,
        }
    }
}

impl Platform for EnvPlatform {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }

    fn process_started(&self, pid: Pid) -> Option<Timestamp> {
        self.inner.process_started(pid)
    }

    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }

    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        let mut cmd = cmd.clone();
        cmd.env_clear = true;
        cmd.env.extend(self.env.iter().cloned());
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
