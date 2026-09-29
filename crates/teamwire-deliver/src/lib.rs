//! Finds a session's current place (ADR 0006: two agreeing views, checked
//! again right before the send) and types one line into it through the
//! host, with the retry rules of ADR 0003.
//!
//! Never: type into a place that only one view names; retry after a stall
//! or a timeout (that outcome is `Unconfirmed`); hold a store transaction
//! across a host call.
//!
//! Invariants: every typed line is a `OneLine`; a "not submitted" host code
//! is retried at most 3 times in about 5 seconds, then `Undelivered`.

use serde::{Deserialize, Serialize};
use teamwire_core::{DeliveryError, DeliveryErrorCode, OneLine, Place};
use teamwire_store::{Ctx, Party};

const NOT_BUILT: &str = "teamwire-deliver is not built yet";

/// What happened to one typed line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeliveryOutcome {
    /// The host accepted the line.
    HandedOff,
    /// Typing stalled or timed out; the line may or may not be on screen.
    Unconfirmed(DeliveryError),
    /// Nothing was typed.
    Undelivered(DeliveryError),
}

/// The place where `party`'s session runs now, agreed by two views.
pub fn locate(_ctx: &Ctx<'_>, _party: &Party) -> Result<Place, DeliveryError> {
    Err(DeliveryError::new(DeliveryErrorCode::HostOther, NOT_BUILT))
}

/// Locates `party` and types `line` into its session.
pub fn deliver_line(_ctx: &Ctx<'_>, _party: &Party, _line: &OneLine) -> DeliveryOutcome {
    DeliveryOutcome::Undelivered(DeliveryError::new(DeliveryErrorCode::HostOther, NOT_BUILT))
}
