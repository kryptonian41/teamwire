//! A message a quiet member must never be shown (MSG-3). The send check
//! refuses a message to a quiet member from anyone but its own lead, but a
//! member can become quiet after a message to it was stored: a relaunch
//! after a failed start may set `quiet` or a quiet kind, and a send can pass
//! its check just before the relaunch and be stored just after it. So every
//! read that shows a member a message asks `ReadTx::hears` again, whatever
//! the time the message was stored.

use teamwire_core::{
    Delivery, DeliveryError, DeliveryErrorCode, DeliveryState, Message, Recipient, Timestamp,
};
use teamwire_store::{Ctx, DeliveryRecord, DeliveryUpdate, StoreError, Tx};

/// The `undelivered` row of a message its recipient may not be shown.
pub(crate) fn is_withheld(delivery: &Delivery) -> bool {
    delivery.state == DeliveryState::Undelivered
        && delivery
            .error
            .as_ref()
            .is_some_and(|e| e.code == DeliveryErrorCode::RecipientNotAllowed)
}

/// Moves the row of a message its recipient may not be shown to
/// `undelivered` with `recipient_not_allowed`: nothing was typed and nothing
/// will be. No resend types such a row, and status lists it nowhere. A row
/// that may already be on the recipient's screen (`handed_off`,
/// `unconfirmed`) or that it read (`acked`) cannot move back, so it stays as
/// it is, and each read leaves it out instead.
pub(crate) fn withhold(tx: &Tx<'_>, at: Timestamp, delivery: &Delivery) -> Result<(), StoreError> {
    if is_withheld(delivery)
        || !matches!(
            delivery.state,
            DeliveryState::Pending | DeliveryState::Undelivered
        )
    {
        return Ok(());
    }
    tx.set_delivery_state(
        at,
        &delivery.message,
        &delivery.recipient,
        &DeliveryUpdate {
            state: DeliveryState::Undelivered,
            route: None,
            error: Some(DeliveryError::new(
                DeliveryErrorCode::RecipientNotAllowed,
                "the recipient is quiet and hears only its own lead",
            )),
            attempts: delivery.attempts,
        },
    )
}

/// Before a message is typed to `recipient`: when the recipient may not be
/// shown it, withholds the row and returns the row as it now stands, so the
/// caller types nothing. A read comes first, so a send to a member that
/// hears the sender takes no extra write lock.
///
/// Typing happens outside any transaction, so a relaunch that commits after
/// this check and before the line is typed is not seen here. The relaunched
/// member's inbox and reminder still leave the message out.
pub(crate) fn withheld_before_typing(
    ctx: &mut Ctx<'_>,
    message: &Message,
    recipient: &Recipient,
) -> Result<Option<DeliveryRecord>, StoreError> {
    if !matches!(recipient, Recipient::Member(_))
        || ctx.store.read(|tx| tx.hears(recipient, &message.sender))?
    {
        return Ok(None);
    }
    let now = ctx.clock.now();
    ctx.store.write(|tx| {
        let reader = tx.reader();
        if reader.hears(recipient, &message.sender)? {
            return Ok(None);
        }
        let Some(row) = reader.delivery(&message.ulid, recipient)? else {
            return Ok(None);
        };
        withhold(tx, now, &row.delivery)?;
        reader.delivery(&message.ulid, recipient)
    })
}
