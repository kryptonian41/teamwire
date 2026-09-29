//! The `Host` trait for tmux: `new-window -e` to spawn with the member's
//! environment, `load-buffer` plus `paste-buffer -p` plus `send-keys Enter`
//! to type, `capture-pane` for the screen, the pane's process tree for its
//! occupant.
//!
//! Never: type into a pane before the screen check of ADR 0006 point 3a
//! (tmux does not guard dialogs); run tmux except through `Platform::run`
//! with a timeout.
//!
//! Invariants: every call names its pane explicitly; nothing depends on the
//! caller's current tmux client.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Host, HostCaps, HostError, HostErrorCode, HostId, Key, Occupant, OneLine, Place, PlaceState,
    Platform, SpawnRequest, SubmitAck,
};

fn not_built() -> HostError {
    HostError::new(HostErrorCode::Other, "teamwire-host-tmux is not built yet")
}

/// The tmux host adapter (id `tmux`).
pub struct Tmux {
    platform: Arc<dyn Platform>,
}

impl Tmux {
    pub const ID: &'static str = "tmux";

    /// tmux's commands run through `platform`.
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self { platform }
    }

    /// The platform tmux's commands run through.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }
}

impl fmt::Debug for Tmux {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tmux").finish_non_exhaustive()
    }
}

impl Host for Tmux {
    fn id(&self) -> HostId {
        HostId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HostCaps {
        HostCaps {
            can_type: true,
            reports_state: false,
            knows_session_of_place: false,
            submit_ack: SubmitAck::None,
            sets_env_at_spawn: true,
            guards_dialogs: false,
        }
    }

    fn spawn(&self, _req: &SpawnRequest, _timeout: Duration) -> Result<Place, HostError> {
        Err(not_built())
    }

    fn submit(&self, _place: &Place, _text: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        Err(not_built())
    }

    fn state(&self, _place: &Place, _timeout: Duration) -> Result<PlaceState, HostError> {
        Err(not_built())
    }

    fn occupant(&self, _place: &Place, _timeout: Duration) -> Result<Occupant, HostError> {
        Err(not_built())
    }

    fn read_tail(
        &self,
        _place: &Place,
        _lines: u16,
        _timeout: Duration,
    ) -> Result<String, HostError> {
        Err(not_built())
    }

    fn send_keys(
        &self,
        _place: &Place,
        _keys: &[Key],
        _timeout: Duration,
    ) -> Result<(), HostError> {
        Err(not_built())
    }

    fn close(&self, _place: &Place, _timeout: Duration) -> Result<(), HostError> {
        Err(not_built())
    }

    fn list(&self, _timeout: Duration) -> Result<Vec<Place>, HostError> {
        Err(not_built())
    }

    fn live_agent_names(&self, _timeout: Duration) -> Result<Vec<String>, HostError> {
        Err(not_built())
    }

    fn place_env_var(&self) -> Option<&'static str> {
        Some("TMUX_PANE")
    }
}
