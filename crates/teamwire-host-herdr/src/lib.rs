//! The `Host` trait for herdr, through herdr's JSON CLI: spawn with
//! `agent start`, type with `agent prompt`, the session in a pane from
//! `agent get`, one workspace per team and one tab per member.
//!
//! Never: match on error text (map `error.code` to `HostErrorCode`); run
//! herdr except through `Platform::run` with a timeout.
//!
//! Invariants: `agent start` is retried only on `agent_pane_busy` and
//! `agent_not_ready`, a bounded number of times; labels are set once.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Host, HostCaps, HostError, HostErrorCode, HostId, Key, Occupant, OneLine, Place, PlaceState,
    Platform, SpawnRequest, SubmitAck,
};

fn not_built() -> HostError {
    HostError::new(HostErrorCode::Other, "teamwire-host-herdr is not built yet")
}

/// The herdr host adapter (id `herdr`).
pub struct Herdr {
    platform: Arc<dyn Platform>,
}

impl Herdr {
    pub const ID: &'static str = "herdr";

    /// herdr's commands run through `platform`.
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self { platform }
    }

    /// The platform herdr's commands run through.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }
}

impl fmt::Debug for Herdr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Herdr").finish_non_exhaustive()
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
            sets_env_at_spawn: false,
            guards_dialogs: true,
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
        Some("HERDR_PANE_ID")
    }
}
