//! The lead's copy of the user's message to a member (ADR 0040): one more
//! delivery row on the same message, added after every rule passed, and
//! typed to the lead as a copy for its information.

use teamwire_core::{
    Actor, DeliveryState, Message, MessageType, OneLine, Recipient, TeamName, Ulid,
};
use teamwire_deliver::{CommsCopy, HeldItem, lead_copy_line, message_line_to, with_held};
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
/// lead's copy, else the line `message_line_to` words. With it, the
/// messages held for a handoff that a reprompt's line names, whose hold
/// ends once the line is typed.
pub(crate) fn line_for(
    ctx: &Ctx<'_>,
    team: &TeamName,
    message: &Message,
    recipient: &Recipient,
    comms: CommsCopy<'_>,
) -> Result<(OneLine, Vec<Ulid>), SendError> {
    if message.sender == Actor::User && *recipient == Recipient::Lead {
        let rows = ctx
            .store
            .read(|tx| tx.deliveries_of(&message.ulid)?.all_or_corrupt())?;
        let all: Vec<Recipient> = rows.into_iter().map(|d| d.delivery.recipient).collect();
        if is_lead_copy(message, &all) {
            let members: Vec<Recipient> = all.into_iter().filter(is_member).collect();
            let line = lead_copy_line(
                team,
                &message.display_id,
                message.kind,
                &message.body,
                &members,
                comms,
            );
            return Ok((line, Vec::new()));
        }
    }
    let line = message_line_to(
        team,
        &message.display_id,
        message.kind,
        &message.sender,
        &message.body,
        recipient,
        comms,
    );
    if message.kind != MessageType::Reprompt || !matches!(recipient, Recipient::Member(_)) {
        return Ok((line, Vec::new()));
    }
    // A reprompt in place of the handoff ends the hold, so its line names
    // what waited for the member (ADR 0043 decision 12).
    let held = held_for(ctx, recipient)?;
    let items: Vec<HeldItem<'_>> = held
        .iter()
        .map(|m| HeldItem {
            id: &m.display_id,
            kind: m.kind,
            sender: &m.sender,
        })
        .collect();
    Ok((
        with_held(line, &items),
        held.iter().map(|m| m.ulid).collect(),
    ))
}

/// The messages held for `recipient` because its newest result asked for a
/// handoff, oldest first.
fn held_for(ctx: &Ctx<'_>, recipient: &Recipient) -> Result<Vec<Message>, SendError> {
    let mut held = ctx.store.read(|tx| {
        let rows = tx.deliveries_for(recipient, &[DeliveryState::Undelivered])?;
        let mut found = Vec::new();
        for row in rows.items.iter().filter(|r| crate::attempt::is_held(r)) {
            if let Some(m) = tx.message(&row.delivery.message)? {
                found.push(m);
            }
        }
        Ok(found)
    })?;
    held.sort_by_key(|m| m.seq);
    Ok(held)
}

fn is_member(r: &Recipient) -> bool {
    matches!(r, Recipient::Member(_))
}
