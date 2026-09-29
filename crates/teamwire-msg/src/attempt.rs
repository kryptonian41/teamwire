//! One delivery attempt to one recipient: locate and type outside any
//! transaction, record the outcome in a short one, and keep a reprompt's
//! generation in step with what is known about its delivery.

use teamwire_core::{
    DeliveryError, DeliveryState, DisplayId, Message, MessageType, Recipient, Refusal, Route,
    TeamName, Timestamp,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line, message_line_to};
use teamwire_store::{Ctx, DeliveryRecord, DeliveryUpdate, Party, Tx};

use crate::{RecipientReport, SendError};

/// The member's generation before a reprompt bumped it, kept to undo the
/// bump when the reprompt is known not to have arrived.
pub(crate) struct Bumped {
    pub(crate) recipient: Recipient,
    generation: u32,
    started_at: Timestamp,
}

/// Starts the recipient's next generation and remembers the one it replaced.
pub(crate) fn bump(
    tx: &Tx<'_>,
    at: Timestamp,
    id: &DisplayId,
    recipient: &Recipient,
) -> Result<Option<Bumped>, teamwire_store::StoreError> {
    let Recipient::Member(role) = recipient else {
        return Ok(None);
    };
    let Some(member) = tx.reader().member(role)? else {
        return Ok(None);
    };
    tx.bump_generation(at, role, id.as_str())?;
    Ok(Some(Bumped {
        recipient: recipient.clone(),
        generation: member.generation,
        started_at: member.gen_started_at,
    }))
}

/// A resume of a reprompt whose earlier failure was undone: bump again, in a
/// transaction that first checks the team is still open.
pub(crate) fn bump_again(
    ctx: &mut Ctx<'_>,
    message: &Message,
    recipient: &Recipient,
) -> Result<Option<Bumped>, SendError> {
    if message.kind != MessageType::Reprompt {
        return Ok(None);
    }
    let now = ctx.clock.now();
    let id = message.display_id.clone();
    let outcome = ctx.store.write(|tx| {
        let open = tx
            .reader()
            .team()?
            .is_some_and(|t| t.state == teamwire_core::TeamState::Open);
        if !open {
            return Ok(None);
        }
        bump(tx, now, &id, recipient).map(Some)
    })?;
    match outcome {
        None => Err(Refusal::TeamClosed.into()),
        Some(bumped) => Ok(bumped),
    }
}

/// Delivers `message` to one recipient, outside any transaction, and records
/// the outcome in a short transaction of its own. A known failure of a
/// reprompt gives the member's earlier generation back; an unconfirmed
/// delivery keeps the new one (INV-5).
pub(crate) fn deliver_one(
    ctx: &mut Ctx<'_>,
    team: &TeamName,
    message: &Message,
    recipient: &Recipient,
    before: DeliveryState,
    attempts: u32,
    bump: Option<&Bumped>,
) -> Result<RecipientReport, SendError> {
    // The lead has no inbox, so its line points at the stored copy.
    let comms_path = ctx
        .home
        .join("teams")
        .join(team.as_str())
        .join("comms")
        .join(format!("{}.md", message.display_id));
    let line = message_line_to(
        team,
        &message.display_id,
        message.kind,
        &message.sender,
        &message.body,
        recipient,
        &comms_path,
    );
    let party = match recipient {
        Recipient::Lead => Party::Lead,
        Recipient::Member(role) => Party::Member(role.clone()),
    };
    let outcome = deliver_line(ctx, &party, &line);
    let (state, route, error) = forward_only(before, outcome);
    let now = ctx.clock.now();
    let update = DeliveryUpdate {
        state,
        route,
        error,
        attempts,
    };
    let record = ctx.store.write(|tx| {
        let reader = tx.reader();
        let current = reader.delivery(&message.ulid, recipient)?;
        // The recipient may have read it, or another command handed it off,
        // while this one was typing. Those states are final here.
        if let Some(c) = &current
            && matches!(
                c.delivery.state,
                DeliveryState::Acked | DeliveryState::HandedOff
            )
        {
            return Ok(current);
        }
        tx.set_delivery_state(now, &message.ulid, recipient, &update)?;
        reader.delivery(&message.ulid, recipient)
    })?;
    let Some(record) = record else {
        return Err(SendError::Internal(format!(
            "the delivery of {} to {recipient} vanished",
            message.display_id
        )));
    };
    if record.delivery.state == DeliveryState::Undelivered
        && let Some(b) = bump
    {
        undo_bump(ctx, b);
    }
    Ok(report_of(&record))
}

/// A delivery state moves forward only: a copy that may have arrived
/// (`unconfirmed`) does not become `undelivered` because a later try failed
/// cleanly, since the earlier try may still have typed it (INV-5).
fn forward_only(
    before: DeliveryState,
    outcome: DeliveryOutcome,
) -> (DeliveryState, Option<Route>, Option<DeliveryError>) {
    match outcome {
        DeliveryOutcome::HandedOff => (DeliveryState::HandedOff, Some(Route::HostTyping), None),
        DeliveryOutcome::Unconfirmed(e) => {
            (DeliveryState::Unconfirmed, Some(Route::HostTyping), Some(e))
        }
        DeliveryOutcome::Undelivered(e) if before == DeliveryState::Unconfirmed => {
            (DeliveryState::Unconfirmed, Some(Route::HostTyping), Some(e))
        }
        DeliveryOutcome::Undelivered(e) => (DeliveryState::Undelivered, None, Some(e)),
    }
}

fn undo_bump(ctx: &mut Ctx<'_>, bumped: &Bumped) {
    let Recipient::Member(role) = &bumped.recipient else {
        return;
    };
    let now = ctx.clock.now();
    let undone = ctx
        .store
        .write(|tx| tx.restore_generation(now, role, bumped.generation, bumped.started_at));
    if let Err(e) = undone {
        // Another reprompt moved the generation on; leave it where it is.
        tracing::warn!(%role, error = %e, "could not give back the generation of a failed reprompt");
    }
}

pub(crate) fn report_of(record: &DeliveryRecord) -> RecipientReport {
    let d = &record.delivery;
    RecipientReport {
        to: d.recipient.clone(),
        state: d.state.into(),
        reason: d
            .error
            .as_ref()
            .filter(|_| {
                matches!(
                    d.state,
                    DeliveryState::Undelivered | DeliveryState::Unconfirmed
                )
            })
            .map(|e| e.code),
        attempts: Some(d.attempts),
    }
}
