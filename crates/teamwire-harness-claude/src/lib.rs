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

mod hook;
mod launch;
mod screen;
mod session;
mod trust;

pub use trust::{StartupPoll, TrustCheck};

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Harness, HarnessCaps, HarnessError, HarnessId, Host, LaunchSpec, Liveness,
    MemberLaunch, OtelConfig, Pid, Place, Platform, Role, ScreenState, SessionId, StartupOutcome,
    TeamName, TrustPolicy, TurnEnd,
};

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
    startup_poll: StartupPoll,
}

impl ClaudeCode {
    pub const ID: &'static str = "claude-code";

    pub fn new(platform: Arc<dyn Platform>, config: ClaudeConfig) -> Self {
        Self {
            platform,
            config,
            startup_poll: StartupPoll::default(),
        }
    }

    /// How long `answer_startup` waits for the prompt or a dialog.
    #[must_use]
    pub fn with_startup_poll(mut self, poll: StartupPoll) -> Self {
        self.startup_poll = poll;
        self
    }

    pub fn config(&self) -> &ClaudeConfig {
        &self.config
    }

    /// The caller's session, read through `get` instead of the process
    /// environment: `CLAUDE_CODE_SESSION_ID`, and the kind from the session
    /// file of `CLAUDE_PID` (`bg` is a background session), else from
    /// `CLAUDE_CODE_SESSION_KIND` or `CLAUDE_CODE_SESSION_ATTENDED=0`.
    /// `current_session` calls this with the real environment.
    pub fn session_from_env(&self, get: impl Fn(&str) -> Option<String>) -> Option<CallerSession> {
        session::caller_from_env(&self.config.claude_dir, get)
    }

    /// What `.claude.json` says about `cwd`, read-only. Launch calls this
    /// before it starts a member, because `answer_startup` sees only the
    /// screen and cannot know the directory.
    pub fn trust_check(&self, cwd: &Path) -> TrustCheck {
        trust::check(&self.config.claude_json, cwd)
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
        self.session_from_env(|name| std::env::var(name).ok())
    }

    fn processes_for_session(
        &self,
        s: &SessionId,
        timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        session::processes(
            self.platform.as_ref(),
            &self.config.claude_dir,
            &self.config.claude_bin,
            s,
            timeout,
        )
    }

    fn is_alive(&self, s: &SessionId, timeout: Duration) -> Result<Liveness, HarnessError> {
        Ok(session::liveness(
            self.platform.as_ref(),
            &self.config.claude_dir,
            &self.config.claude_bin,
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

    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        trust::answer_startup(host, place, policy, self.startup_poll)
    }

    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        // The launcher's own attributes are kept and ours are appended.
        let existing = std::env::var("OTEL_RESOURCE_ATTRIBUTES").ok();
        launch::otel_vars(team, role, base, existing.as_deref())
    }
}
