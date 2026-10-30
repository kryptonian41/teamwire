//! Unread messages: the in-turn reminder, and the list the Stop hook adds to
//! its hold note. Each message id is reminded at most once (ADR 0003 point 8).

use teamwire_core::{DeliveryState, Recipient, Role, Ulid};
use teamwire_deliver::{AttemptHistory, line_holds_whole};
use teamwire_store::{Ctx, StoreError};

use crate::note::unread_note;

/// A message the member has not read and has not been reminded of.
pub(crate) struct Unread {
    pub(crate) message: Ulid,
    pub(crate) display_id: String,
}

/// Deliveries to `me` that are not read (`acked`) and whose reminder has not
/// been sent. A `handed_off` message whose whole text the typed line carried
/// is left out: the member already has all of it (ADR 0021). A `pending` delivery is still being sent by its sender, so it
/// counts only once it is stale: a sender that crashed after the commit
/// would otherwise leave the message unnamed for good (ADR 0003 point 8).
pub(crate) fn unread(ctx: &Ctx<'_>, me: &Role) -> Result<Vec<Unread>, StoreError> {
    ctx.store.read(|tx| {
        let recipient = Recipient::Member(me.clone());
        let deliveries = tx.deliveries_for(
            &recipient,
            &[
                DeliveryState::Pending,
                DeliveryState::HandedOff,
                DeliveryState::Unconfirmed,
                DeliveryState::Undelivered,
            ],
        )?;
        let now = ctx.clock.now();
        let mut out = Vec::new();
        for d in deliveries.items.iter().filter(|d| d.reminded_at.is_none()) {
            let message = match tx.message(&d.delivery.message) {
                Ok(found) => found,
                Err(StoreError::Corrupt(why)) => {
                    // One damaged message must not hide the unread ones after
                    // it, nor the member's own result the hook also delivers.
                    tracing::warn!(message = %d.delivery.message, %why, "skipping a damaged message");
                    continue;
                }
                Err(e) => return Err(e),
            };
            if let Some(message) = message {
                if d.delivery.state == DeliveryState::HandedOff && line_holds_whole(&message.body) {
                    continue;
                }
                // A damaged event row about the message must not stop the
                // hook, which also delivers the member's own result: the
                // message is treated as not stale and left out.
                if d.delivery.state == DeliveryState::Pending
                    && !AttemptHistory::read(tx, &message, &recipient)
                        .map(|a| a.is_stale(now))
                        .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "could not tell whether a pending message is stale");
                        false
                    })
                {
                    continue;
                }
                out.push(Unread {
                    message: message.ulid,
                    display_id: message.display_id.to_string(),
                });
            }
        }
        Ok(out)
    })
}

/// Marks each delivery to `me` that a sender still holds `pending`
/// (`delivery.recipient_stopped`, ADR 0028). `unread` leaves such a message
/// to its sender, who may type it; if that sender then finds the member
/// busy and gives up, no later stop would name it, so the sender reads the
/// mark and tries again. The list and the marks are one write: a sender
/// that records `undelivered` either commits first, and this stop's hold
/// names the message, or commits after, and sees the mark.
///
/// Every turn end marks, the one after a hold too, since that one names
/// nothing. Any store error is logged: a hook never fails a turn.
pub(crate) fn mark_stopped(ctx: &mut Ctx<'_>, me: &Role) {
    let now = ctx.clock.now();
    let recipient = Recipient::Member(me.clone());
    let marked = ctx.store.write(|tx| {
        let reader = tx.reader();
        let pending = reader.deliveries_for(&recipient, &[DeliveryState::Pending])?;
        for d in &pending.items {
            let Some(message) = reader.message(&d.delivery.message)? else {
                continue;
            };
            // A stale row is a crashed sender's; `unread` names it instead.
            // One whose age cannot be read is marked: a needless mark costs
            // the sender one more try, a missing one loses the message.
            if !AttemptHistory::read(&reader, &message, &recipient).is_ok_and(|a| a.is_stale(now)) {
                tx.record_recipient_stopped(now, &message.ulid, &recipient)?;
            }
        }
        Ok(())
    });
    if let Err(e) = marked {
        tracing::warn!(error = %e, role = %me, "could not mark the deliveries still being sent");
    }
}

/// The reminder of unread message ids for the in-turn hook, once per id.
///
/// Any store error means no reminder: a hook never fails a turn.
pub fn on_tool_use(ctx: &mut Ctx<'_>, me: &Role) -> Option<String> {
    let found = match unread(ctx, me) {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e, "could not read the unread messages");
            return None;
        }
    };
    if found.is_empty() {
        return None;
    }
    let at = ctx.clock.now();
    let recipient = Recipient::Member(me.clone());
    let marked = ctx.store.write(|tx| {
        let mut ids = Vec::new();
        for u in &found {
            // A message read since the list was made is not reminded.
            if tx.mark_reminded(at, &u.message, &recipient).is_ok() {
                ids.push(u.display_id.clone());
            }
        }
        Ok(ids)
    });
    match marked {
        Ok(ids) if !ids.is_empty() => Some(unread_note(&ids)),
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(error = %e, "could not mark the reminders");
            None
        }
    }
}
