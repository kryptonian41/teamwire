//! The `Harness` trait for the Codex CLI from `OpenAI` (ADR 0023): launch and
//! resume argv built from `-c` overrides, the member's token file, `Stop`,
//! `PostToolUse` and `SessionStart` hook input, the TUI screen, session
//! (rollout) files and thread locks for liveness, and the lead plugin that
//! `teamwire install` adds.
//!
//! Never: write under `CODEX_HOME` (`~/.codex`) or edit its `config.toml`;
//! change Codex's setup other than through `codex plugin`; put a token in an
//! argv; start a member without `--no-daemon`.
//!
//! Invariants: every Codex path comes from `CodexConfig`, so tests point
//! them at a temporary directory; a member's identity reaches its shell
//! tools, hooks and MCP server through `-c` values that name the 0600 token
//! file, never the token itself.

mod hook;
mod launch;
pub mod lead_setup;
mod screen;
mod session;
mod startup;

pub use startup::StartupPoll;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Harness, HarnessCaps, HarnessError, HarnessId, Host, LaunchSpec, Liveness,
    MemberLaunch, OneLine, OtelConfig, Pid, Place, Platform, Role, ScreenState, SessionId,
    StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};

/// Where Codex keeps its files, and how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexConfig {
    /// `CODEX_HOME`, else `~/.codex`, or `TEAMWIRE_CODEX_HOME` in tests.
    /// Holds `sessions/`, `thread-writer-locks/` and `config.toml`; teamwire
    /// only reads it.
    pub codex_home: PathBuf,
    /// The `codex` program to start.
    pub codex_bin: String,
}

/// The Codex CLI harness adapter (id `codex`).
pub struct CodexCli {
    platform: Arc<dyn Platform>,
    config: CodexConfig,
    startup_poll: StartupPoll,
}

impl CodexCli {
    pub const ID: &'static str = "codex";

    pub fn new(platform: Arc<dyn Platform>, config: CodexConfig) -> Self {
        Self {
            platform,
            config,
            startup_poll: StartupPoll::default(),
        }
    }

    /// How long `answer_startup` waits for the composer or a dialog.
    #[must_use]
    pub fn with_startup_poll(mut self, poll: StartupPoll) -> Self {
        self.startup_poll = poll;
        self
    }

    pub fn config(&self) -> &CodexConfig {
        &self.config
    }
}

impl fmt::Debug for CodexCli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexCli")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Harness for CodexCli {
    fn id(&self) -> HarnessId {
        HarnessId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: true,
            in_turn_hook: true,
            mcp_client: true,
            // Codex picks the thread id itself; the SessionStart hook records it.
            choose_session_id: false,
            can_push: false,
            session_env_via_config: true,
            // Codex has no tool that streams a command's output to the model.
            reads_watch_lines: false,
        }
    }

    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        launch::build(&self.config.codex_bin, m, &launch::Start::New)
    }

    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        launch::build(&self.config.codex_bin, m, &launch::Start::Resume(s))
    }

    fn current_session(&self) -> Option<CallerSession> {
        self.caller_session(&|name| std::env::var(name).ok())
    }

    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        session::caller_session(&self.config.codex_home, get)
    }

    fn lead_resume_argv(&self, s: &SessionId) -> Result<Vec<String>, HarnessError> {
        Ok(vec![
            self.config.codex_bin.clone(),
            "--no-daemon".to_owned(),
            "resume".to_owned(),
            s.as_str().to_owned(),
        ])
    }

    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        Ok(session::session_cwd(&self.config.codex_home, s))
    }

    fn processes_for_session(
        &self,
        s: &SessionId,
        timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        session::processes(self.platform.as_ref(), &self.config.codex_home, s, timeout)
    }

    fn is_alive(&self, s: &SessionId, timeout: Duration) -> Result<Liveness, HarnessError> {
        Ok(session::liveness(
            self.platform.as_ref(),
            &self.config.codex_home,
            s,
            timeout,
        ))
    }

    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        hook::parse_turn_end(stdin)
    }

    fn hold_reply(&self, note: &str) -> Vec<u8> {
        hook::hold_reply(note)
    }

    fn screen_state(&self, tail: &str) -> ScreenState {
        screen::classify(tail)
    }

    fn line_pending(&self, tail: &str, line: &OneLine) -> bool {
        screen::line_pending(tail, line)
    }

    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        startup::answer_startup(host, place, policy, self.startup_poll)
    }

    fn otel_env(
        &self,
        _team: &TeamName,
        _role: &Role,
        _base: &OtelConfig,
    ) -> Vec<(String, String)> {
        Vec::new()
    }
}
