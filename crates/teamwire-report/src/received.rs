//! `teamwire received` (ADR 0024, ADR 0043): the lead says it read a member's
//! result, or a message to the lead, from its file, after the typing failed
//! or stalled. Nothing is typed again.

use serde::{Deserialize, Serialize};
use teamwire_core::{DeliveryState, DisplayId, Recipient, ReportState, Role};
use teamwire_store::{Ctx, StoreError, Tx};

/// What `received` did for one role or message id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Received {
    /// The member's newest result, number `n`, is now `delivered`.
    Marked { n: u32 },
    /// The member's newest result is not `undelivered` or `unconfirmed`, or
    /// it has none. `state` is that result's state.
    NotWaiting { state: Option<ReportState> },
    /// No member has this role.
    NoMember,
    /// The lead's delivery of this message is now `acked`.
    Read,
    /// The message was typed into the lead's pane; nothing changed.
    AlreadyDelivered,
    /// The lead had already read the message; nothing changed.
    AlreadyRead,
    /// The team has no message with this id.
    NoMessage,
    /// The message did not go to the lead.
    NotToLead,
    /// A role of the team has the name too, so it is not clear which one
    /// the lead meant; nothing was recorded.
    RoleAndMessage,
}

/// Whether `name` reads as a message display id: `m` and digits.
fn looks_like_message_id(name: &str) -> bool {
    name.strip_prefix('m')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Marks each role's newest result received, and each message id's
/// delivery to the lead read. Only the newest result counts: an older one
/// was replaced, and its file overwritten, by the newer one. A name is a
/// message id only when it is `m` and digits and no role of the team has
/// that name; when a role has it too, nothing is recorded for it.
pub fn mark_received(
    ctx: &mut Ctx<'_>,
    roles: &[Role],
) -> Result<Vec<(Role, Received)>, StoreError> {
    let at = ctx.clock.now();
    ctx.store.write(|tx| {
        let mut out = Vec::new();
        for role in roles {
            let member = tx.reader().member(role)?.is_some();
            let outcome = if looks_like_message_id(role.as_str()) {
                if member {
                    Received::RoleAndMessage
                } else {
                    mark_message_read(tx, at, role.as_str())?
                }
            } else if !member {
                Received::NoMember
            } else {
                match tx.reader().latest_completion(role)? {
                    Some(r) if tx.mark_result_received(at, &r.completion.ulid)? => {
                        Received::Marked { n: r.completion.n }
                    }
                    newest => Received::NotWaiting {
                        state: newest.map(|r| r.completion.report_state),
                    },
                }
            };
            out.push((role.clone(), outcome));
        }
        Ok(out)
    })
}

/// Moves the lead's delivery of message `id` to `acked`, only from a state
/// in which the lead never saw it typed: `ack_delivery` would also move a
/// `handed_off` row.
fn mark_message_read(
    tx: &Tx<'_>,
    at: teamwire_core::Timestamp,
    id: &str,
) -> Result<Received, StoreError> {
    let Ok(id) = DisplayId::parse(id) else {
        return Ok(Received::NoMessage);
    };
    let Some(message) = tx.reader().message_by_display_id(&id)? else {
        return Ok(Received::NoMessage);
    };
    let Some(row) = tx.reader().delivery(&message.ulid, &Recipient::Lead)? else {
        return Ok(Received::NotToLead);
    };
    Ok(match row.delivery.state {
        DeliveryState::Undelivered | DeliveryState::Unconfirmed | DeliveryState::Pending => {
            tx.ack_delivery(at, &message.ulid, &Recipient::Lead)?;
            Received::Read
        }
        DeliveryState::HandedOff => Received::AlreadyDelivered,
        DeliveryState::Acked => Received::AlreadyRead,
    })
}
