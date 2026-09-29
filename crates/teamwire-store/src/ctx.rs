//! The handle every service call takes, and how it finds a party's adapters.

use std::fmt;
use std::path::Path;

use teamwire_core::{
    Clock, Config, DeliveryError, DeliveryErrorCode, Harness, HarnessId, Host, HostId, MemberState,
    Platform, Registry, Role,
};

use crate::{Store, StoreError};

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
    /// The harness and host recorded for `party`, from the registry. The
    /// lead's come from the team row, a member's from its own row; a removed
    /// or unknown member is `recipient_removed`, and an id this binary does
    /// not have is `unknown_adapter`.
    pub fn adapters(&self, party: &Party) -> Result<(&dyn Harness, &dyn Host), DeliveryError> {
        let (harness, host) = self.recorded_ids(party)?;
        Ok((self.registry.harness(&harness)?, self.registry.host(&host)?))
    }

    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    fn recorded_ids(&self, party: &Party) -> Result<(HarnessId, HostId), DeliveryError> {
        let unreadable = |e: StoreError| {
            DeliveryError::new(
                DeliveryErrorCode::UnknownAdapter,
                format!("the recorded adapters could not be read: {e}"),
            )
        };
        match party {
            Party::Lead => {
                let team = self.store.read(|tx| tx.team()).map_err(unreadable)?;
                team.map(|t| (t.lead_harness, t.lead_host)).ok_or_else(|| {
                    DeliveryError::new(DeliveryErrorCode::UnknownAdapter, "the team has no row")
                })
            }
            Party::Member(role) => {
                let member = self.store.read(|tx| tx.member(role)).map_err(unreadable)?;
                match member {
                    Some(m) if m.state != MemberState::Removed => Ok((m.harness, m.host)),
                    Some(_) => Err(DeliveryError::new(
                        DeliveryErrorCode::RecipientRemoved,
                        format!("member {role} was removed"),
                    )),
                    None => Err(DeliveryError::new(
                        DeliveryErrorCode::RecipientRemoved,
                        format!("no member {role}"),
                    )),
                }
            }
        }
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
