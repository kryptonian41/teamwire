//! The `Host` trait for herdr, through herdr's JSON CLI: spawn with
//! `agent start`, type with `agent prompt`, the session in a pane from
//! `agent get`, the screen from `agent read` (the one command that prints
//! plain text, not JSON), one workspace per team and one tab per member.
//!
//! Never: match on error text (map `error.code` to `HostErrorCode`); run
//! herdr except through `Platform::run` with a timeout.
//!
//! Invariants: `agent start` is retried only on `agent_pane_busy`, a bounded
//! number of times and never past the caller's timeout; `agent_not_ready`
//! counts as started; labels are set once.

mod cli;
mod errors;
mod shapes;
mod spawn;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Address, Clock, Host, HostCaps, HostError, HostErrorCode, HostId, Key, Occupant, OneLine,
    Place, PlaceState, Platform, SessionId, SpawnRequest, SubmitAck,
};

/// How often `agent start` tries, and how long it waits between tries,
/// when herdr says the pane is busy (LCH-13).
const START_TRIES: u32 = 8;
const START_RETRY_DELAY: Duration = Duration::from_millis(1500);

/// The herdr host adapter (id `herdr`).
///
/// herdr `error.code` to `HostErrorCode` (anything not listed is `Other`,
/// with the code and message in the detail):
///
/// | herdr code | `HostErrorCode` |
/// | --- | --- |
/// | `agent_pane_busy` | `Busy` |
/// | `agent_not_ready` | `NotReady` |
/// | `agent_blocked` | `Blocked` |
/// | `agent_prompt_stalled` | `Stalled` |
/// | `timeout` | `Timeout` |
/// | `agent_not_found`, `pane_not_found`, `agent_pane_not_found`, `workspace_not_found`, `tab_not_found` | `NotFound` |
/// | `invalid_agent_argument`, `invalid_agent_name`, `invalid_agent_timeout`, `empty_agent_prompt`, `invalid_params` | `BadArgument` |
/// | `agent_name_taken` | `BadArgument`, detail starting `agent_name_taken:` |
///
/// A syntax error is usage text on stderr with exit 2, not JSON; it maps to
/// `BadArgument`. A call that runs out of time maps to `Timeout`.
pub struct Herdr {
    platform: Arc<dyn Platform>,
    start_tries: u32,
    start_retry_delay: Duration,
    /// Measures the caller's timeout across the calls of one `spawn`.
    clock: Arc<dyn Clock>,
}

impl Herdr {
    pub const ID: &'static str = "herdr";

    /// herdr's commands run through `platform`.
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            start_tries: START_TRIES,
            start_retry_delay: START_RETRY_DELAY,
            clock: Arc::new(teamwire_platform::SystemClock),
        }
    }

    /// Measures time with `clock`, so a test moves the deadline of a `spawn`.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Changes how `agent start` retries a busy or not-ready pane. Tests use
    /// a short delay; a run of zero tries still makes one call.
    #[must_use]
    pub fn with_start_retry(mut self, tries: u32, delay: Duration) -> Self {
        self.start_tries = tries.max(1);
        self.start_retry_delay = delay;
        self
    }

    /// The platform herdr's commands run through.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }

    fn call(&self, args: &[&str], timeout: Duration) -> Result<serde_json::Value, HostError> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        cli::call(self.platform.as_ref(), &args, timeout)
    }

    fn call_text(&self, args: &[&str], timeout: Duration) -> Result<String, HostError> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        cli::call_text(self.platform.as_ref(), &args, timeout)
    }

    fn agent(&self, place: &Place, timeout: Duration) -> Result<shapes::AgentInfo, HostError> {
        shapes::agent_info(self.call(&["agent", "get", &place.id], timeout)?)
    }

    fn agents(&self, timeout: Duration) -> Result<Vec<shapes::AgentInfo>, HostError> {
        shapes::agent_list(self.call(&["agent", "list"], timeout)?)
    }
}

impl fmt::Debug for Herdr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Herdr").finish_non_exhaustive()
    }
}

/// herdr's name for a key.
fn key_name(key: Key) -> String {
    match key {
        Key::Enter => "enter".to_owned(),
        Key::Escape => "esc".to_owned(),
        Key::Tab => "tab".to_owned(),
        Key::Backspace => "backspace".to_owned(),
        Key::Up => "up".to_owned(),
        Key::Down => "down".to_owned(),
        Key::Left => "left".to_owned(),
        Key::Right => "right".to_owned(),
        Key::CtrlC => "ctrl+c".to_owned(),
        Key::Char(c) => c.to_string(),
    }
}

impl Host for Herdr {
    fn id(&self) -> HostId {
        HostId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HostCaps {
        HostCaps {
            can_type: true,
            reports_state: true,
            knows_session_of_place: true,
            submit_ack: SubmitAck::Submitted,
            // herdr's `--env` exists on `workspace create`, `tab create` and
            // `pane split`, but a member is started with `agent start` in a
            // pane that already exists, and that command takes no `--env`.
            // Identity reaches the member through the harness config (ADR 0008).
            sets_env_at_spawn: false,
            guards_dialogs: true,
        }
    }

    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
        spawn::spawn(self, req, timeout)
    }

    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError> {
        self.call(&["agent", "prompt", &place.id, text.as_str()], timeout)
            .map(drop)
    }

    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        match self.agent(place, timeout) {
            Ok(info) => Ok(info.place_state()),
            Err(e) if e.code == HostErrorCode::NotFound => Ok(PlaceState::Gone),
            Err(e) => Err(e),
        }
    }

    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
        let info = self.agent(place, timeout)?;
        Ok(Occupant {
            session: info.session_value().and_then(|v| SessionId::parse(v).ok()),
            pids: Vec::new(),
        })
    }

    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        let lines = lines.to_string();
        self.call_text(
            &[
                "agent",
                "read",
                &place.id,
                "--source",
                "recent-unwrapped",
                "--lines",
                &lines,
            ],
            timeout,
        )
    }

    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        if keys.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = keys.iter().copied().map(key_name).collect();
        let mut args = vec!["agent", "send-keys", place.id.as_str()];
        args.extend(names.iter().map(String::as_str));
        self.call(&args, timeout).map(drop)
    }

    fn close(&self, place: &Place, expected: &Address, timeout: Duration) -> Result<(), HostError> {
        // Two herdr calls run one after the other, so each gets half: the
        // whole call stays inside the caller's `timeout`. A pane with no
        // agent fails `agent get` with a not-found code and is left alone.
        let each = timeout / 2;
        let info = self.agent(place, each)?;
        if info.name.as_deref() != Some(expected.as_str()) {
            return Err(HostError::new(
                HostErrorCode::NotFound,
                format!("{} holds no agent named {expected}", place.id),
            ));
        }
        self.call(&["pane", "close", &place.id], each).map(drop)
    }

    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        let host = self.id();
        Ok(self
            .agents(timeout)?
            .into_iter()
            .filter(|a| a.name.as_deref().is_some_and(|n| !n.is_empty()))
            .filter_map(|a| a.pane_id)
            .map(|id| Place {
                host: host.clone(),
                id,
            })
            .collect())
    }

    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        Ok(self
            .agents(timeout)?
            .into_iter()
            .filter_map(|a| a.name)
            .filter(|n| !n.is_empty())
            .collect())
    }

    fn place_env_var(&self) -> Option<&'static str> {
        Some("HERDR_PANE_ID")
    }
}
