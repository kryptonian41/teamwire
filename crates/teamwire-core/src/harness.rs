//! The `Harness` trait: one agent program (Claude Code first), and the
//! records a harness produces to start a member.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::OtelConfig;
use crate::host::{Host, HostError, PlaceState};
use crate::member::MemberKind;
use crate::names::{Address, HarnessId, Pid, Place, Role, SessionId, TeamName};
use crate::sanitize::OneLine;
use crate::secret::RedactedEnv;
use crate::strings::string_enum;

pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> HarnessCaps;
    /// argv, env and files to start one member. Files are written by launch
    /// under the team directory before spawn; nothing outside it.
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError>;
    /// argv to resume an existing member session with the same per-run
    /// configuration (shown by `status --role`, never run by teamwire).
    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError>;
    /// The caller's own session, read from its environment.
    fn current_session(&self) -> Option<CallerSession>;
    fn processes_for_session(
        &self,
        s: &SessionId,
        timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError>;
    fn is_alive(&self, s: &SessionId, timeout: Duration) -> Result<Liveness, HarnessError>;
    /// Parse turn-end hook stdin into the neutral form.
    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError>;
    /// Render the hook reply that holds the member once with `note`.
    fn hold_reply(&self, note: &str) -> Vec<u8>;
    /// Classify a screen tail: at the input prompt, showing a dialog, or unknown.
    fn screen_state(&self, tail: &str) -> ScreenState;
    /// Handle a startup dialog on the member's screen, if the harness has one.
    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError>;
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)>;
    /// Put one line into a session without a host. Only called when
    /// `capabilities().can_push`; the default returns `Unsupported`.
    fn push(&self, _s: &SessionId, _line: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        Err(HostError::unsupported())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "a capability record is a set of independent yes/no facts (ADR 0004 point 5)"
)]
pub struct HarnessCaps {
    pub turn_end_hook: bool,
    pub in_turn_hook: bool,
    pub mcp_client: bool,
    pub choose_session_id: bool,
    pub can_push: bool,
    /// Environment for the session can be set in a per-run
    /// config file (Claude Code `settings.json` `env`).
    pub session_env_via_config: bool,
}

/// A turn-end hook's input, in neutral form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnEnd {
    pub session: SessionId,
    pub subagent: bool,
    pub hold_already_active: bool,
    pub final_message: Option<String>,
    pub transcript: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Liveness {
    Alive(PlaceState),
    Dead,
    Unknown,
}

string_enum! {
    pub enum ScreenState {
        InputPrompt => "input_prompt",
        Dialog => "dialog",
        Unknown => "unknown",
    }
}

string_enum! {
    pub enum StartupOutcome {
        NoDialog => "no_dialog",
        AnsweredPlainTrust => "answered_plain_trust",
        RefusedEscalated => "refused_escalated",
    }
}

string_enum! {
    pub enum SessionKind {
        Interactive => "interactive",
        Background => "background",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallerSession {
    pub id: SessionId,
    pub kind: SessionKind,
}

string_enum! {
    /// From the spec's `accept_trust`.
    pub enum TrustPolicy {
        Accept => "accept",
        Refuse => "refuse",
    }
}

/// Everything a harness needs to build one member's start. `identity` holds
/// the member's token, so `Debug` hides secret values.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberLaunch {
    pub team: TeamName,
    pub role: Role,
    pub address: Address,
    pub kind: MemberKind,
    pub cwd: PathBuf,
    pub session_id: SessionId,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `$TEAMWIRE_HOME/teams/<team>/members/<role>`.
    pub member_dir: PathBuf,
    pub contract_path: PathBuf,
    pub identity: Vec<(String, String)>,
    pub otel: OtelConfig,
    pub teamwire_bin: PathBuf,
}

/// What the harness returns. Files are written by launch (mode given) before spawn.
/// `env` and the files can hold the token, so `Debug` hides secret values and
/// prints each file's size, never its bytes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub argv: Vec<String>,
    /// Honoured only when the host has `sets_env_at_spawn`.
    pub env: Vec<(String, String)>,
    /// For example `settings.json` and `mcp.json`.
    pub files: Vec<LaunchFile>,
    pub identity_channel: IdentityChannel,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

string_enum! {
    /// How a member's identity variables reach it (ADR 0008).
    pub enum IdentityChannel {
        HarnessConfig => "harness_config",
        HostEnv => "host_env",
    }
}

/// What the turn-end hook tells the harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnEndReply {
    Continue,
    /// Carries the note.
    Hold(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code}: {detail}")]
pub struct HarnessError {
    pub code: HarnessErrorCode,
    pub detail: String,
}

impl HarnessError {
    pub fn new(code: HarnessErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The adapter does not offer this call.
    pub fn unsupported() -> Self {
        Self::new(
            HarnessErrorCode::Unsupported,
            "not supported by this adapter",
        )
    }
}

string_enum! {
    pub enum HarnessErrorCode {
        /// The harness's session list or file does not know the session.
        NotFound => "not_found",
        /// A listing or read ran past its timeout.
        Timeout => "timeout",
        /// Hook input or a harness file could not be parsed.
        BadInput => "bad_input",
        /// The member cannot be started as asked (for example no identity channel).
        Refused => "refused",
        /// The call went through the host, and the host failed.
        Host => "host",
        Io => "io",
        Unsupported => "unsupported",
        Other => "other",
    }
}

impl From<HostError> for HarnessError {
    fn from(e: HostError) -> Self {
        Self::new(HarnessErrorCode::Host, e.to_string())
    }
}

impl fmt::Debug for MemberLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemberLaunch")
            .field("team", &self.team)
            .field("role", &self.role)
            .field("address", &self.address)
            .field("kind", &self.kind)
            .field("cwd", &self.cwd)
            .field("session_id", &self.session_id)
            .field("model", &self.model)
            .field("effort", &self.effort)
            .field("member_dir", &self.member_dir)
            .field("contract_path", &self.contract_path)
            .field("identity", &RedactedEnv(&self.identity))
            .field("otel", &self.otel)
            .field("teamwire_bin", &self.teamwire_bin)
            .finish()
    }
}

impl fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchSpec")
            .field("argv", &self.argv)
            .field("env", &RedactedEnv(&self.env))
            .field("files", &self.files)
            .field("identity_channel", &self.identity_channel)
            .finish()
    }
}

impl fmt::Debug for LaunchFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchFile")
            .field("path", &self.path)
            .field("bytes", &format_args!("<{} bytes>", self.bytes.len()))
            .field("mode", &format_args!("{:o}", self.mode))
            .finish()
    }
}
