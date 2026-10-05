//! `resend`: an explicit retry of one stored message (ADR 0003 point 2a,
//! MSG-19). It never makes a message. It adds a delivery attempt to the
//! existing rows and skips the burst and repeat limits.

use teamwire_core::{
    Actor, DeliveryState, DisplayId, Message, Recipient, Refusal, TeamState, Timestamp,
};
use teamwire_deliver::is_pending_stale;
use teamwire_store::{Ctx, DeliveryRecord};

use crate::attempt::{bump_again, deliver_one, earlier_bump, report_of};
use crate::send::{read_team, record_refusal, survive_store_error, team_name};
use crate::{RecipientReport, RecipientState, SendError, SendReport};

/// Retries `id` to its `undelivered`, `unconfirmed` or stale `pending`
/// recipients, or only to those named in `to`. Only the sender or the lead
/// may resend. A recipient that was reached, or is not yet resendable, is
/// reported as `skipped`.
pub fn resend(
    ctx: &mut Ctx<'_>,
    actor: &Actor,
    id: &DisplayId,
    to: Option<&[Recipient]>,
) -> Result<SendReport, SendError> {
    let message = ctx
        .store
        .read(|tx| tx.message_by_display_id(id))?
        .ok_or_else(|| SendError::InvalidArgument(format!("no message {id}")))?;
    let rows = ctx
        .store
        .read(|tx| tx.deliveries_of(&message.ulid)?.all_or_corrupt())?;
    if *actor != Actor::Lead && *actor != message.sender {
        let recipients: Vec<Recipient> =
            rows.iter().map(|r| r.delivery.recipient.clone()).collect();
        record_refusal(
            ctx,
            Some(actor),
            &Refusal::NotResendable,
            Some(message.kind),
            &recipients,
        );
        return Err(Refusal::NotResendable.into());
    }
    if let Some(named) = to
        && let Some(stranger) = named
            .iter()
            .find(|r| !rows.iter().any(|row| &row.delivery.recipient == *r))
    {
        return Err(SendError::InvalidArgument(format!(
            "{stranger} is not a recipient of {id}"
        )));
    }
    // A closing or closed team types nothing, as for `send` (CLS-3).
    if !team_is_open(ctx)? {
        let recipients: Vec<Recipient> =
            rows.iter().map(|r| r.delivery.recipient.clone()).collect();
        record_refusal(
            ctx,
            Some(actor),
            &Refusal::TeamClosed,
            Some(message.kind),
            &recipients,
        );
        return Err(Refusal::TeamClosed.into());
    }
    let now = ctx.clock.now();
    let wanted =
        |row: &DeliveryRecord| to.is_none_or(|named| named.contains(&row.delivery.recipient));
    let mut chosen: Vec<&DeliveryRecord> = Vec::new();
    for row in rows.iter().filter(|r| wanted(r)) {
        if resendable(ctx, row, &message, now)? {
            chosen.push(row);
        }
    }
    if chosen.is_empty() {
        return Err(SendError::NotResendable(format!(
            "no recipient of {id} is undelivered, unconfirmed or pending for over 60 seconds"
        )));
    }
    let team = team_name(ctx)?;
    let mut deliveries: Vec<RecipientReport> = Vec::new();
    for row in rows.iter().filter(|r| wanted(r)) {
        if !chosen
            .iter()
            .any(|c| c.delivery.recipient == row.delivery.recipient)
        {
            deliveries.push(RecipientReport {
                state: RecipientState::Skipped,
                ..report_of(row)
            });
            continue;
        }
        let result = retry(ctx, actor, &team, &message, row);
        deliveries.push(survive_store_error(
            result,
            &row.delivery.recipient,
            row.delivery.attempts + 1,
        )?);
    }
    Ok(SendReport {
        id: message.ulid,
        display_id: message.display_id,
        seq: message.seq,
        deliveries,
        duplicate_of_key: false,
    })
}

/// A `pending` row is resendable only after its latest attempt began over
/// `PENDING_STALE` ago: a crash left it, and a send that is still delivering
/// must not be typed a second time.
fn resendable(
    ctx: &Ctx<'_>,
    row: &DeliveryRecord,
    message: &Message,
    now: Timestamp,
) -> Result<bool, SendError> {
    Ok(match row.delivery.state {
        DeliveryState::Undelivered | DeliveryState::Unconfirmed => true,
        DeliveryState::Pending => ctx
            .store
            .read(|tx| is_pending_stale(tx, message, &row.delivery.recipient, now))?,
        DeliveryState::HandedOff | DeliveryState::Acked => false,
    })
}

/// One more attempt for one row: the attempt is counted and written as
/// `delivery.resent` first, then the line is delivered.
fn retry(
    ctx: &mut Ctx<'_>,
    actor: &Actor,
    team: &teamwire_core::TeamName,
    message: &Message,
    row: &DeliveryRecord,
) -> Result<RecipientReport, SendError> {
    let recipient = &row.delivery.recipient;
    let before = row.delivery.state;
    let now = ctx.clock.now();
    // The team can start closing after the first check, so the attempt is
    // counted only inside a transaction that sees it still open.
    let attempts = ctx.store.write(|tx| {
        let open = tx
            .reader()
            .team()?
            .is_some_and(|t| t.state == TeamState::Open);
        if open {
            tx.begin_resend(now, &message.ulid, recipient).map(Some)
        } else {
            Ok(None)
        }
    })?;
    let Some(attempts) = attempts else {
        record_refusal(
            ctx,
            Some(actor),
            &Refusal::TeamClosed,
            Some(message.kind),
            std::slice::from_ref(recipient),
        );
        return Err(Refusal::TeamClosed.into());
    };
    // A reprompt that failed cleanly was undone, so its next try starts the
    // generation again. One that may have arrived kept its generation. One
    // left `pending` by a dead sender kept a bump that nothing has undone.
    // The bump comes after the attempt is counted, so a failed count leaves
    // the generation alone.
    let bump = match before {
        DeliveryState::Undelivered => bump_again(ctx, message, recipient)?,
        DeliveryState::Pending => earlier_bump(ctx, message, recipient)?,
        DeliveryState::HandedOff | DeliveryState::Unconfirmed | DeliveryState::Acked => None,
    };
    deliver_one(
        ctx,
        team,
        message,
        recipient,
        before,
        attempts,
        bump.as_ref(),
    )
}

fn team_is_open(ctx: &Ctx<'_>) -> Result<bool, SendError> {
    Ok(read_team(ctx)?.is_some_and(|t| t.state == TeamState::Open))
}
