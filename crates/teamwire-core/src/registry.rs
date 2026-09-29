//! The adapter registry: string ids to the adapters this binary has.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use thiserror::Error;

use crate::harness::Harness;
use crate::host::Host;
use crate::message::{DeliveryError, DeliveryErrorCode};
use crate::names::{HarnessId, HostId};

/// Filled by `teamwire-cli`'s wiring with every adapter this binary has.
/// Adapter crates never register themselves into core types.
#[derive(Default)]
pub struct Registry {
    harnesses: BTreeMap<String, Arc<dyn Harness>>,
    hosts: BTreeMap<String, Arc<dyn Host>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryError {
    #[error("an adapter with id {0:?} is already registered")]
    Duplicate(String),
    #[error("this binary has no harness adapter {0}")]
    UnknownHarness(HarnessId),
    #[error("this binary has no host adapter {0}")]
    UnknownHost(HostId),
}

impl From<RegistryError> for DeliveryError {
    fn from(e: RegistryError) -> Self {
        Self::new(DeliveryErrorCode::UnknownAdapter, e.to_string())
    }
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuses a second adapter with the same id.
    pub fn add_harness(&mut self, h: Arc<dyn Harness>) -> Result<(), RegistryError> {
        let id = h.id().as_str().to_owned();
        if self.harnesses.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.harnesses.insert(id, h);
        Ok(())
    }

    /// Refuses a second adapter with the same id.
    pub fn add_host(&mut self, h: Arc<dyn Host>) -> Result<(), RegistryError> {
        let id = h.id().as_str().to_owned();
        if self.hosts.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.hosts.insert(id, h);
        Ok(())
    }

    pub fn harness(&self, id: &HarnessId) -> Result<&dyn Harness, RegistryError> {
        self.harnesses
            .get(id.as_str())
            .map(AsRef::as_ref)
            .ok_or_else(|| RegistryError::UnknownHarness(id.clone()))
    }

    pub fn host(&self, id: &HostId) -> Result<&dyn Host, RegistryError> {
        self.hosts
            .get(id.as_str())
            .map(AsRef::as_ref)
            .ok_or_else(|| RegistryError::UnknownHost(id.clone()))
    }

    /// The registered harness ids, sorted.
    pub fn harness_ids(&self) -> Vec<&str> {
        self.harnesses.keys().map(String::as_str).collect()
    }

    /// The registered host ids, sorted.
    pub fn host_ids(&self) -> Vec<&str> {
        self.hosts.keys().map(String::as_str).collect()
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("harnesses", &self.harness_ids())
            .field("hosts", &self.host_ids())
            .finish()
    }
}
