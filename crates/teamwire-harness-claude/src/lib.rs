//! The `Harness` trait for Claude Code: launch argv, per-run `settings.json`
//! and `mcp.json`, session files and `claude agents --json` for liveness,
//! background-session detection, Stop and `PostToolUse` hook input, and the
//! trust dialog.
//!
//! Never: write anything under the user's `~/.claude/` or `~/.claude.json`
//! (both are read-only here); answer the escalated trust dialog; register a
//! hook or MCP server globally.
//!
//! Invariants: every Claude path comes from `ClaudeConfig`, so tests point
//! them at a temporary directory; the member's identity travels in the
//! `env` key of its own `settings.json`.

mod launch;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Harness, HarnessCaps, HarnessError, HarnessErrorCode, HarnessId, Host,
    LaunchSpec, Liveness, MemberLaunch, OtelConfig, Pid, Place, Platform, Role, ScreenState,
    SessionId, StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};

fn not_built() -> HarnessError {
    HarnessError::new(
        HarnessErrorCode::Other,
        "teamwire-harness-claude is not built yet",
    )
}

/// Where Claude Code keeps its files, and how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeConfig {
    /// `~/.claude`, or `TEAMWIRE_CLAUDE_DIR` in tests. Holds `sessions/`.
    pub claude_dir: PathBuf,
    /// `~/.claude.json`: read for `hasTrustDialogAccepted`, never written.
    pub claude_json: PathBuf,
    /// The `claude` program to start.
    pub claude_bin: String,
}

/// The Claude Code harness adapter (id `claude-code`).
pub struct ClaudeCode {
    platform: Arc<dyn Platform>,
    config: ClaudeConfig,
}

impl ClaudeCode {
    pub const ID: &'static str = "claude-code";

    pub fn new(platform: Arc<dyn Platform>, config: ClaudeConfig) -> Self {
        Self { platform, config }
    }

    pub fn config(&self) -> &ClaudeConfig {
        &self.config
    }

    /// The platform Claude's commands run through.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }
}

impl fmt::Debug for ClaudeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeCode")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Harness for ClaudeCode {
    fn id(&self) -> HarnessId {
        HarnessId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: true,
            in_turn_hook: true,
            mcp_client: true,
            choose_session_id: true,
            can_push: false,
            session_env_via_config: true,
        }
    }

    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        launch::build(&self.config.claude_bin, m, &launch::SessionArg::New)
    }

    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        launch::build(&self.config.claude_bin, m, &launch::SessionArg::Resume(s))
    }

    fn current_session(&self) -> Option<CallerSession> {
        None
    }

    fn processes_for_session(
        &self,
        _s: &SessionId,
        _timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        Err(not_built())
    }

    fn is_alive(&self, _s: &SessionId, _timeout: Duration) -> Result<Liveness, HarnessError> {
        Err(not_built())
    }

    fn parse_turn_end(&self, _stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        Err(not_built())
    }

    fn hold_reply(&self, _note: &str) -> Vec<u8> {
        Vec::new()
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
        Err(not_built())
    }

    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        // The launcher's own attributes are kept and ours are appended.
        let existing = std::env::var("OTEL_RESOURCE_ATTRIBUTES").ok();
        launch::otel_vars(team, role, base, existing.as_deref())
    }
}
