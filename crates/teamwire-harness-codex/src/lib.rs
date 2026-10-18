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

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Harness, HarnessCaps, HarnessError, HarnessId, Host, LaunchSpec, Liveness,
    MemberLaunch, OtelConfig, Pid, Place, Platform, Role, ScreenState, SessionId, StartupOutcome,
    TeamName, TrustPolicy, TurnEnd,
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
}

impl CodexCli {
    pub const ID: &'static str = "codex";

    pub fn new(platform: Arc<dyn Platform>, config: CodexConfig) -> Self {
        Self { platform, config }
    }

    pub fn config(&self) -> &CodexConfig {
        &self.config
    }

    /// The platform Codex's commands run through.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
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
        None
    }

    fn processes_for_session(
        &self,
        _s: &SessionId,
        _timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        Ok(Vec::new())
    }

    fn is_alive(&self, _s: &SessionId, _timeout: Duration) -> Result<Liveness, HarnessError> {
        Ok(Liveness::Unknown)
    }

    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        hook::parse_turn_end(stdin)
    }

    fn hold_reply(&self, note: &str) -> Vec<u8> {
        hook::hold_reply(note)
    }

    fn screen_state(&self, _tail: &str) -> ScreenState {
        ScreenState::Unknown
    }

    fn answer_startup(
        &self,
        _host: &dyn Host,
        _place: &Place,
        _policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        Ok(StartupOutcome::NoDialog)
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
