//! When a `pending` delivery counts as left behind. A sender that is still
//! delivering holds its rows `pending` too, so age alone is not enough: the
//! clock starts at the latest attempt, and a line typed twice is worse than
//! a line that waits another minute.

use std::time::Duration;

use teamwire_core::{Event, EventType, Message, Recipient, Timestamp};
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
    let latest = tx
        .events_about(&message.ulid)?
        .all_or_corrupt()?
        .into_iter()
        .filter(|e| e.kind == EventType::DeliveryResent && is_for(e, recipient))
        .map(|e| e.time)
        .max();
    Ok(latest.map_or(message.created_at, |t| t.max(message.created_at)))
}

/// The event's `to` names the recipient. An event written before `to` was
/// recorded names a member by its role and the lead by no role; an agent in
/// another team also has no role, so only `to` can tell it from the lead.
fn is_for(event: &Event, recipient: &Recipient) -> bool {
    if let Some(to) = event.data.get("to").and_then(|v| v.as_str()) {
        return to == recipient.as_str();
    }
    match recipient {
        Recipient::Lead => event.role.is_none(),
        Recipient::Member(role) => event.role.as_ref() == Some(role),
        Recipient::Other(_) => false,
    }
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
