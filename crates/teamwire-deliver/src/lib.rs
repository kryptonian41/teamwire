//! Finds a session's current place (ADR 0006: two agreeing views, checked
//! again right before the send) and types one line into it through the
//! host, with the retry rules of ADR 0003.
//!
//! Never: type into a place that only one view names; retry after a stall
//! or a timeout (that outcome is `Unconfirmed`); hold a store transaction
//! across a host call; type into a session that shows a dialog.
//!
//! Invariants: every typed line is a `OneLine`; a "not submitted" host code
//! is retried at most 3 times in about 5 seconds, then `Undelivered`.

mod budget;
mod deliver;
mod lines;
mod locate;

use serde::{Deserialize, Serialize};
use teamwire_core::DeliveryError;

pub use deliver::{deliver_line, deliver_line_within};
pub use lines::{message_line, message_line_to, result_line};
pub use locate::locate;

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
