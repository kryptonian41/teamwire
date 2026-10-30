//! The `Harness` trait for Claude Code: launch argv, per-run `settings.json`
//! and `mcp.json`, session files and `claude agents --json` for liveness,
//! background-session detection, Stop and `PostToolUse` hook input, the
//! trust dialog, and the teamwire plugin that `teamwire install` writes.
//!
//! Never: write under the user's `~/.claude/` except deleting an ADR 0012
//! skill file during install or uninstall; write `~/.claude.json` at all;
//! change Claude Code's setup other than through `claude plugin` and
//! `claude mcp remove` (ADR 0018); answer the escalated trust dialog; register a member's hook
//! or MCP server globally.
//!
//! Invariants: every Claude path comes from `ClaudeConfig`, so tests point
//! them at a temporary directory; the member's identity travels in the
//! `env` key of its own `settings.json`.

mod cwd;
mod fill;
mod hook;
mod launch;
mod lead_setup;
mod screen;
mod session;
mod trust;

pub use lead_setup::{
    Change, LeadSetupError, LeadSetupState, McpEntry, McpState, SKILL_TEXT, Shadow, SkillState,
    edited_skill, lead_setup_state, mcp_state, shadowing_entries, skill_state,
};
pub use teamwire_core::screen::StartupPoll;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Fill, Harness, HarnessCaps, HarnessError, HarnessId, Host, LaunchSpec,
    LeadSetup, Liveness, MemberLaunch, OneLine, OtelConfig, Pid, Place, Platform, Role,
    ScreenState, SessionId, StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};

/// Where Claude Code keeps its files, and how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeConfig {
    /// `~/.claude`, `CLAUDE_CONFIG_DIR`, or `TEAMWIRE_CLAUDE_DIR` in tests.
    /// Holds `sessions/`, `skills/`, `plugins/` and `settings.json`.
    pub claude_dir: PathBuf,
    /// `~/.claude.json`, or `.claude.json` inside `claude_dir` when that is
    /// set from the environment. Read for the lead's MCP entry; teamwire never
    /// writes it.
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
            reads_watch_lines: true,
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

    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        self.session_from_env(get)
    }

    fn lead_setup(&self) -> Option<&dyn LeadSetup> {
        Some(self)
    }

    fn lead_resume_argv(&self, s: &SessionId) -> Result<Vec<String>, HarnessError> {
        Ok(vec![
            self.config.claude_bin.clone(),
            "--resume".to_owned(),
            s.as_str().to_owned(),
        ])
    }

    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        Ok(cwd::session_cwd(&self.config.claude_dir, s))
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
        screen::delivery_state(tail)
    }

    fn line_pending(&self, tail: &str, line: &OneLine) -> bool {
        screen::line_pending(tail, line)
    }

    fn reads_input_box(&self) -> bool {
        true
    }

    fn box_holds_text(&self, tail: &str) -> bool {
        screen::box_holds_text(tail)
    }

    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        trust::answer_startup(host, place, policy, self.startup_poll)
    }

    fn context_fill(
        &self,
        s: &SessionId,
        transcript: Option<&Path>,
    ) -> Result<Option<Fill>, HarnessError> {
        Ok(fill::context_fill(&self.config.claude_dir, s, transcript))
    }

    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        // The launcher's own attributes are kept and ours are appended.
        let existing = std::env::var("OTEL_RESOURCE_ATTRIBUTES").ok();
        launch::otel_vars(team, role, base, existing.as_deref())
    }
}
