//! The `Host` trait: the place programs run (herdr, tmux, none).

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::names::{Address, HostId, Pid, Place, SessionId};
use crate::sanitize::OneLine;
use crate::secret::RedactedEnv;
use crate::strings::string_enum;

pub trait Host: Send + Sync {
    fn id(&self) -> HostId;
    fn capabilities(&self) -> HostCaps;
    /// Create a new place for a member and run `argv` in it.
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError>;
    /// Type `text` and submit it. `Ok` means the host accepted it, no more.
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError>;
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError>;
    /// Who occupies `place`: a session id if the host knows it, else process ids.
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError>;
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError>;
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError>;
    fn close(&self, place: &Place, timeout: Duration) -> Result<(), HostError>;
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError>;
    /// Names of every live agent the host knows, from any tool (for LCH-25).
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError>;
    /// The environment variable a process uses to name its own place, if any
    /// (herdr: "`HERDR_PANE_ID`").
    fn place_env_var(&self) -> Option<&'static str>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "a capability record is a set of independent yes/no facts (ADR 0004 point 5)"
)]
pub struct HostCaps {
    pub can_type: bool,
    pub reports_state: bool,
    pub knows_session_of_place: bool,
    pub submit_ack: SubmitAck,
    /// The host can give the started program environment
    /// variables (tmux `new-window -e`). herdr cannot.
    pub sets_env_at_spawn: bool,
    /// The host itself refuses to type into a session that
    /// shows a dialog (herdr `agent_blocked`). tmux does not.
    pub guards_dialogs: bool,
}

string_enum! {
    /// What `Ok(())` from `Host::submit` proves.
    pub enum SubmitAck {
        None => "none",
        Submitted => "submitted",
        StateChange => "state_change",
    }
}

string_enum! {
    pub enum PlaceState {
        Idle => "idle",
        Working => "working",
        Blocked => "blocked",
        Gone => "gone",
        Unknown => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occupant {
    pub session: Option<SessionId>,
    pub pids: Vec<Pid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code}: {detail}")]
pub struct HostError {
    pub code: HostErrorCode,
    pub detail: String,
}

impl HostError {
    pub fn new(code: HostErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The adapter does not offer this call.
    pub fn unsupported() -> Self {
        Self::new(HostErrorCode::Unsupported, "not supported by this adapter")
    }
}

string_enum! {
    pub enum HostErrorCode {
        NotReady => "not_ready",
        Busy => "busy",
        Blocked => "blocked",
        NotFound => "not_found",
        Stalled => "stalled",
        Timeout => "timeout",
        BadArgument => "bad_argument",
        Unsupported => "unsupported",
        Other => "other",
    }
}

/// What a host needs to start one member in a new place. `Debug` hides
/// secret values in `env`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub name: Address,
    pub title: String,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl fmt::Debug for SpawnRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpawnRequest")
            .field("name", &self.name)
            .field("title", &self.title)
            .field("cwd", &self.cwd)
            .field("argv", &self.argv)
            .field("env", &RedactedEnv(&self.env))
            .finish()
    }
}

/// One key for `Host::send_keys`. Each host adapter maps these to its own
/// key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    CtrlC,
    /// One printable character, such as the `1` that picks a dialog option.
    Char(char),
}

/// The host for "no multiplexer" (id "none"). Every capability false; every
/// writing call returns `HostError` { code: `NotFound`, .. } and does nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHost;

impl NoHost {
    pub const ID: &'static str = "none";

    fn not_found() -> HostError {
        HostError::new(HostErrorCode::NotFound, "no host: there is no place")
    }
}

impl Host for NoHost {
    fn id(&self) -> HostId {
        HostId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HostCaps {
        HostCaps {
            can_type: false,
            reports_state: false,
            knows_session_of_place: false,
            submit_ack: SubmitAck::None,
            sets_env_at_spawn: false,
            guards_dialogs: false,
        }
    }

    fn spawn(&self, _req: &SpawnRequest, _timeout: Duration) -> Result<Place, HostError> {
        Err(Self::not_found())
    }

    fn submit(&self, _place: &Place, _text: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        Err(Self::not_found())
    }

    fn state(&self, _place: &Place, _timeout: Duration) -> Result<PlaceState, HostError> {
        Err(Self::not_found())
    }

    fn occupant(&self, _place: &Place, _timeout: Duration) -> Result<Occupant, HostError> {
        Err(Self::not_found())
    }

    fn read_tail(
        &self,
        _place: &Place,
        _lines: u16,
        _timeout: Duration,
    ) -> Result<String, HostError> {
        Err(Self::not_found())
    }

    fn send_keys(
        &self,
        _place: &Place,
        _keys: &[Key],
        _timeout: Duration,
    ) -> Result<(), HostError> {
        Err(Self::not_found())
    }

    fn close(&self, _place: &Place, _timeout: Duration) -> Result<(), HostError> {
        Err(Self::not_found())
    }

    fn list(&self, _timeout: Duration) -> Result<Vec<Place>, HostError> {
        Ok(Vec::new())
    }

    fn live_agent_names(&self, _timeout: Duration) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }

    fn place_env_var(&self) -> Option<&'static str> {
        None
    }
}
