//! `inbox` and `ack`: a member reads its messages once (DEL-13). Reading
//! marks a delivery `acked`, the only way it reaches that state (MSG-11).

use serde_json::json;
use teamwire_core::{DeliveryState, DisplayId, EventType, Recipient, Role};
use teamwire_store::Ctx;

use crate::{AckOutcome, AckResult, InboxItem, SendError};

/// The states that still count as unread: everything but `acked`.
const UNREAD: [DeliveryState; 4] = [
    DeliveryState::Pending,
    DeliveryState::HandedOff,
    DeliveryState::Unconfirmed,
    DeliveryState::Undelivered,
];

/// Every state, for `inbox --all`.
const EVERY: [DeliveryState; 5] = [
    DeliveryState::Pending,
    DeliveryState::HandedOff,
    DeliveryState::Unconfirmed,
    DeliveryState::Undelivered,
    DeliveryState::Acked,
];

/// The caller's messages, oldest first. Without `all`, only unread ones,
/// and each is marked `acked` in the same transaction that returns it, so a
/// message is never marked read without having been returned. With `all`,
/// read messages too, and nothing is marked.
///
/// A body that no longer matches its stored hash is returned with `hash_ok:
/// false`, and a `store.error` event says so. A row that cannot be parsed
/// at all is left out and recorded the same way.
pub fn inbox(ctx: &mut Ctx<'_>, me: &Role, all: bool) -> Result<Vec<InboxItem>, SendError> {
    let now = ctx.clock.now();
    let recipient = Recipient::Member(me.clone());
    let states: &[DeliveryState] = if all { &EVERY } else { &UNREAD };
    Ok(ctx.store.write(|tx| {
        let reader = tx.reader();
        let found = reader.inbox(&recipient, states)?;
        for bad in &found.bad {
            tx.record_event(
                now,
                EventType::StoreError,
                Some(me),
                None,
                &json!({ "op": "inbox", "code": "corrupt", "table": bad.table, "key": bad.key }),
            )?;
        }
        let mut items = Vec::with_capacity(found.items.len());
        for entry in found.items {
            let m = &entry.message;
            if !entry.hash_ok {
                tx.record_event(
                    now,
                    EventType::StoreError,
                    Some(me),
                    Some(&m.ulid),
                    &json!({ "op": "inbox", "code": "hash_mismatch", "id": m.display_id.as_str() }),
                )?;
            }
            let re = match &m.re {
                Some(ulid) => reader.message(ulid)?.map(|q| q.display_id),
                None => None,
            };
            items.push(InboxItem {
                display_id: m.display_id.clone(),
                id: m.ulid,
                from: m.sender.clone(),
                kind: m.kind,
                time: m.created_at,
                re,
                commit: m.commit.clone(),
                text: m.body.clone(),
                hash_ok: entry.hash_ok,
            });
            if !all {
                tx.ack_delivery(now, &m.ulid, &recipient)?;
            }
        }
        Ok(items)
    })?)
}

/// Marks messages read without returning them. An id that names no message,
/// or a message that was not sent to the caller, is `unknown`; one already
/// read is `already`.
pub fn ack(ctx: &mut Ctx<'_>, me: &Role, ids: &[DisplayId]) -> Result<Vec<AckResult>, SendError> {
    let now = ctx.clock.now();
    let recipient = Recipient::Member(me.clone());
    Ok(ctx.store.write(|tx| {
        let reader = tx.reader();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let outcome = match reader.message_by_display_id(id)? {
                Some(m) if reader.delivery(&m.ulid, &recipient)?.is_some() => {
                    if tx.ack_delivery(now, &m.ulid, &recipient)? {
                        AckOutcome::Acked
                    } else {
                        AckOutcome::Already
                    }
                }
                _ => AckOutcome::Unknown,
            };
            out.push(AckResult {
                id: id.clone(),
                outcome,
            });
        }
        Ok(out)
    })?)
}
