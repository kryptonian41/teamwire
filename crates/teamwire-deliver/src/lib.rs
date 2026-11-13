//! Finds a session's current place (ADR 0006: two agreeing views, checked
//! again right before the send) and types one line into it through the
//! host, with the retry rules of ADR 0003. Reads what one message's events
//! say about its latest attempt to one recipient (`AttemptHistory`), so the
//! senders, the hooks and the lead's views share one reading of it.
//!
//! Never: type into a place that only one view names; retry after a stall
//! or a timeout (that outcome is `Unconfirmed`); hold a store transaction
//! across a host call; type into a session that shows a dialog.
//!
//! Invariants: every typed line is a `OneLine`; a "not submitted" host code
//! is tried at most 3 times (2 retries) in about 5 seconds, then `Undelivered`.

mod budget;
mod deliver;
mod history;
mod lease;
mod lines;
mod locate;
mod unsent;

use serde::{Deserialize, Serialize};
use teamwire_core::DeliveryError;

pub use deliver::{deliver_line, deliver_line_within};
pub use history::{AttemptHistory, PENDING_STALE};
pub use lease::{WATCH_LEASE_STALE, watch_lease_is_dead};
pub use lines::{
    CommsCopy, HeldItem, WatchText, agent_text, lead_copy_line, line_holds_whole, message_line,
    message_line_to, result_line, result_path, result_written, watch_text, with_held,
};
pub use locate::{locate, locate_session};

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
