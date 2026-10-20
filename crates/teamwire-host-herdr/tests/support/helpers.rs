//! Shared by the integration tests: a `Herdr` whose `herdr` is `fake-herdr`.
#![expect(
    clippy::expect_used,
    reason = "helpers in an integration-test file unwrap and index; a failure there fails the test"
)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Address, Command, Pid, Platform, PlatformError, RunOutput, SleepHold, SpawnRequest,
};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, HerdrFaults, LoggedCall};
use teamwire_host_herdr::Herdr;

/// The budget of one herdr call. A loaded machine takes seconds to start a
/// freshly built test binary, and the limit only guards against a hang.
pub(crate) const T: Duration = Duration::from_secs(60);

/// Runs every command with `env` on top of the inherited environment, which
/// is how `PATH` finds the fake `herdr`. Everything else is the real platform.
struct EnvPlatform {
    inner: Arc<dyn Platform>,
    env: Vec<(String, String)>,
}

impl Platform for EnvPlatform {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        let mut cmd = cmd.clone();
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

pub(crate) struct World {
    pub(crate) env: FakeHerdrEnv,
    pub(crate) herdr: Herdr,
}

impl World {
    pub(crate) fn new() -> Self {
        Self::with_faults(None)
    }

    pub(crate) fn with_faults(faults: Option<&HerdrFaults>) -> Self {
        let mut env = FakeHerdrEnv::new().expect("fake world");
        env.set_script(&ClaudeScript::default()).expect("script");
        if let Some(f) = faults {
            env.set_faults(f).expect("faults");
        }
        let platform: Arc<dyn Platform> = Arc::new(EnvPlatform {
            inner: teamwire_platform::native(),
            env: env.env(),
        });
        let herdr = Herdr::new(platform).with_start_retry(8, Duration::from_millis(5));
        Self { env, herdr }
    }

    pub(crate) fn request(&self, name: &str, title: &str) -> SpawnRequest {
        let settings = self
            .env
            .write_file(&format!("{name}-settings.json"), r#"{"env":{},"hooks":{}}"#)
            .expect("settings");
        SpawnRequest {
            name: Address::parse(name).expect("address"),
            title: title.to_owned(),
            cwd: self.env.root().to_path_buf(),
            argv: vec![
                "claude".into(),
                "-n".into(),
                name.into(),
                "--settings".into(),
                settings.display().to_string(),
            ],
            env: vec![],
        }
    }

    /// The logged herdr calls whose first two words are `command`.
    pub(crate) fn calls_of(&self, command: &str) -> Vec<LoggedCall> {
        self.env
            .calls()
            .into_iter()
            .filter(|c| c.command() == command)
            .collect()
    }
}
