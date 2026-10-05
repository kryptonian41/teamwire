//! When a `pending` delivery counts as left behind. A sender that is still
//! delivering holds its rows `pending` too, so age alone is not enough: the
//! clock starts at the latest attempt, and a line typed twice is worse than
//! a line that waits another minute.

use std::time::Duration;

use teamwire_core::{EventType, Message, Recipient, Timestamp};
use teamwire_store::{ReadTx, StoreError};

/// A `pending` delivery is stale only after this long without a new attempt.
pub const PENDING_STALE: Duration = Duration::from_secs(60);

/// When the latest attempt at `message` for `recipient` began: the message's
/// creation, or the latest `delivery.resent` for that recipient if later.
/// The deliveries table keeps no attempt time of its own.
pub fn attempt_started(
    tx: &ReadTx<'_>,
    message: &Message,
    recipient: &Recipient,
) -> Result<Timestamp, StoreError> {
    let role = match recipient {
        Recipient::Lead => None,
        Recipient::Member(role) => Some(role),
    };
    let latest = tx
        .events_about(&message.ulid)?
        .all_or_corrupt()?
        .into_iter()
        .filter(|e| e.kind == EventType::DeliveryResent && e.role.as_ref() == role)
        .map(|e| e.time)
        .max();
    Ok(latest.map_or(message.created_at, |t| t.max(message.created_at)))
}

/// True when the attempt at `message` for `recipient` began over
/// `PENDING_STALE` ago.
pub fn is_pending_stale(
    tx: &ReadTx<'_>,
    message: &Message,
    recipient: &Recipient,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let started = attempt_started(tx, message, recipient)?;
    Ok(now.saturating_since(started) > PENDING_STALE)
}
