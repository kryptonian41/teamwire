//! The lead's copy of the user's message to a member (ADR 0040): one more
//! delivery row on the same message, added after every rule passed, and
//! typed to the lead as a copy for its information.

use teamwire_core::{Actor, Message, OneLine, Recipient, TeamName};
use teamwire_deliver::{CommsCopy, lead_copy_line, message_line_to};
use teamwire_store::Ctx;

use crate::SendError;

/// The recipients to store: `to`, and for the user's message to a member
/// also the lead, so the lead never misses an instruction the user gave.
/// No rule counts the copy or refuses it.
pub(crate) fn with_lead_copy(actor: &Actor, to: &[Recipient]) -> Vec<Recipient> {
    let mut all = to.to_vec();
    if adds_lead_copy(actor, to) {
        all.push(Recipient::Lead);
    }
    all
}

/// Whether a message from `actor` to `to`, each named as its own team names
/// it, also goes to the lead as a copy. The lead's row is then not one the
/// sender addressed, so `send`'s exit code leaves it out. The rules refuse a
/// user's message that names the lead together with a member.
pub fn adds_lead_copy(actor: &Actor, to: &[Recipient]) -> bool {
    *actor == Actor::User && to.iter().any(is_member)
}

/// Whether a stored message's lead row is a copy: the message is the
/// user's and also went to a member. The user's note to the lead goes as
/// its own message, so such a lead row is never one the user addressed.
pub(crate) fn is_lead_copy(message: &Message, recipients: &[Recipient]) -> bool {
    message.sender == Actor::User && recipients.iter().any(is_member)
}

/// The line typed to `recipient` for `message`: the copy line for the
/// lead's copy, else the line `message_line_to` words.
pub(crate) fn line_for(
    ctx: &Ctx<'_>,
    team: &TeamName,
    message: &Message,
    recipient: &Recipient,
    comms: CommsCopy<'_>,
) -> Result<OneLine, SendError> {
    if message.sender == Actor::User && *recipient == Recipient::Lead {
        let rows = ctx
            .store
            .read(|tx| tx.deliveries_of(&message.ulid)?.all_or_corrupt())?;
        let all: Vec<Recipient> = rows.into_iter().map(|d| d.delivery.recipient).collect();
        if is_lead_copy(message, &all) {
            let members: Vec<Recipient> = all.into_iter().filter(is_member).collect();
            return Ok(lead_copy_line(
                team,
                &message.display_id,
                message.kind,
                &message.body,
                &members,
                comms,
            ));
        }
    }
    Ok(message_line_to(
        team,
        &message.display_id,
        message.kind,
        &message.sender,
        &message.body,
        recipient,
        comms,
    ))
}

fn is_member(r: &Recipient) -> bool {
    matches!(r, Recipient::Member(_))
}
