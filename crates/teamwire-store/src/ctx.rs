//! The handle every service call takes, and how it finds a party's adapters.

use std::fmt;
use std::path::Path;

use teamwire_core::{
    Clock, Config, DeliveryError, DeliveryErrorCode, Harness, Host, Platform, Registry, Role,
};

use crate::Store;

const NOT_BUILT: &str = "not built yet";

/// What every service call takes: the store, the adapters, the clock and
/// the config. The CLI and the MCP server build one per command.
pub struct Ctx<'a> {
    pub home: &'a Path,
    pub store: &'a mut Store,
    pub registry: &'a Registry,
    pub platform: &'a dyn Platform,
    pub clock: &'a dyn Clock,
    pub config: &'a Config,
}

/// Who a delivery or lookup is for. Resolves to that party's own adapters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Party {
    Lead,
    Member(Role),
}

impl Ctx<'_> {
    /// The harness and host recorded for `party`, from the registry.
    pub fn adapters(&self, _party: &Party) -> Result<(&dyn Harness, &dyn Host), DeliveryError> {
        Err(DeliveryError::new(
            DeliveryErrorCode::UnknownAdapter,
            NOT_BUILT,
        ))
    }
}

impl fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ctx")
            .field("home", &self.home)
            .field("store", &self.store)
            .field("registry", &self.registry)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
