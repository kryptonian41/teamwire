//! The `Platform`, `SleepHold` and `Clock` traits: the operating system and
//! time, behind traits so that tests replace both.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::names::Pid;
use crate::secret::RedactedEnv;
use crate::strings::string_enum;
use crate::timestamp::Timestamp;

pub trait Platform: Send + Sync {
    fn pid_alive(&self, pid: Pid) -> bool;
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError>;
    /// Run in a new process group; on timeout kill the group; exit 124.
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError>;
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError>;
    fn sleep_hold(&self) -> Option<&dyn SleepHold>;
    fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError>;
    fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError>;
}

pub trait SleepHold {
    fn find_existing(&self, lead_pid: Pid) -> Option<HoldInfo>;
    fn start(&self, lead_pid: Pid) -> Result<HoldInfo, PlatformError>;
    /// Kills only if `hold.pid` is still an untimed hold on `lead_pid`.
    fn release(&self, hold: &HoldInfo, lead_pid: Pid) -> Result<ReleaseOutcome, PlatformError>;
}

pub trait Clock: Send + Sync {
    /// Wall clock, stored.
    fn now(&self) -> Timestamp;
    /// Timeouts inside a process.
    fn monotonic(&self) -> Instant;
}

/// A process to run through `Platform::run`: plain data, so a fake platform
/// can record it and a test can compare it. `Debug` hides secret values in
/// `env` and prints only the size of `stdin`.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Set in the child on top of the inherited environment, or on top of an
    /// empty one when `env_clear` is true.
    pub env: Vec<(String, String)>,
    pub env_clear: bool,
    /// Written to the child's stdin, which is then closed. `None` gives the
    /// child an empty stdin.
    pub stdin: Option<Vec<u8>>,
}

impl Command {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn env_clear(mut self) -> Self {
        self.env_clear = true;
        self
    }

    #[must_use]
    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Command")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("cwd", &self.cwd)
            .field("env", &RedactedEnv(&self.env))
            .field("env_clear", &self.env_clear)
            .field(
                "stdin",
                &self.stdin.as_ref().map(|b| format!("<{} bytes>", b.len())),
            )
            .finish()
    }
}

/// What a finished (or timed-out) process left behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutput {
    /// The exit code; 124 when the timeout killed the group, and 128 plus
    /// the signal number when a signal ended the process.
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

impl RunOutput {
    /// The exit code `run` reports for a timeout, as `timeout(1)` does.
    pub const TIMEOUT_EXIT: i32 = 124;

    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.timed_out
    }
}

/// A sleep hold process (`caffeinate` on macOS), as recorded on the team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldInfo {
    pub pid: Pid,
    /// True when teamwire started this hold; a hold the user started is
    /// reused but never killed (SLP-1).
    pub owned: bool,
}

string_enum! {
    pub enum ReleaseOutcome {
        /// The hold was ours and still an untimed hold on the lead; it was killed.
        Released => "released",
        /// The hold was not started by teamwire; it was left running.
        NotOwned => "not_owned",
        /// The process had already exited.
        AlreadyGone => "already_gone",
        /// The pid is now some other process; it was left alone.
        NotAHold => "not_a_hold",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code}: {detail}")]
pub struct PlatformError {
    pub code: PlatformErrorCode,
    pub detail: String,
}

impl PlatformError {
    pub fn new(code: PlatformErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// This platform does not offer the call (for example the sleep hold on Linux).
    pub fn unsupported() -> Self {
        Self::new(
            PlatformErrorCode::Unsupported,
            "not supported on this platform",
        )
    }
}

string_enum! {
    pub enum PlatformErrorCode {
        NotFound => "not_found",
        PermissionDenied => "permission_denied",
        /// The disk is full.
        NoSpace => "no_space",
        Io => "io",
        Unsupported => "unsupported",
        Other => "other",
    }
}
