//! Delivery to agents in another team (ADR 0016). The message is stored in
//! both teams under one ULID: the sender's copy, whose rows name `role@team`,
//! and the recipient team's copy, whose rows name its own parties. The order
//! is the sender's copy (already committed by the caller), the recipient
//! team's copy, the typing, the outcome there, and last the outcome mirrored
//! onto the sender's row. Each write is its own short transaction in one
//! store; no transaction is open while a line is typed, and no two stores
//! are ever locked at once.
//!
//! The recipient team's copy is the one that counts. The sender's row shows
//! the last outcome the sender saw and never becomes `acked`. Every step can
//! run again: the copy is found by its ULID, and a row already handed off is
//! never typed a second time, so a resend repairs a send that died anywhere.

use teamwire_core::{
    DeliveryError, DeliveryErrorCode, DeliveryState, Message, Recipient, Remote, Route, TeamName,
};
use teamwire_store::{
    Ctx, DeliveryRecord, DeliveryUpdate, ReceivedMessage, Store, StoreError, StoredMessage,
};

use crate::attempt::{deliver_one, report_of};
use crate::send::write_copy;
use crate::{RecipientReport, SendError};

/// One row of the sender's copy to try now, with the attempt number it is
/// recorded under in the sender's team.
pub(crate) struct Try<'r> {
    pub(crate) row: &'r DeliveryRecord,
    pub(crate) attempts: u32,
}

/// Delivers `message`, stored in team `own`, to the rows in `tries`, all of
/// which name agents of `team`. `all` is every row of the message for that
/// team, so the copy there is complete even when only some are tried. One
/// result per try, in order. `op` names the command in the `store.error` row
/// that the recipient team's store keeps of its first failure, written
/// before that store is dropped, whichever way this returns.
pub(crate) fn deliver_to_team(
    ctx: &mut Ctx<'_>,
    own: &TeamName,
    message: &Message,
    team: &TeamName,
    all: &[&DeliveryRecord],
    tries: &[Try<'_>],
    op: &str,
) -> Vec<Result<RecipientReport, SendError>> {
    let mut store = match Store::open_existing(ctx.home, team) {
        Ok(store) => store,
        // The team is gone: closed and cleaned up, or archived.
        Err(StoreError::NoDatabase { .. }) => {
            return tries
                .iter()
                .map(|t| mirror(ctx, message, t, gone(team)))
                .collect();
        }
        Err(e) => return tries.iter().map(|_| Err(e.clone().into())).collect(),
    };
    let results = deliver_in(ctx, &mut store, own, message, team, all, tries);
    store.record_failure(ctx.clock.now(), op);
    results
}

/// The part of `deliver_to_team` that runs with the recipient team's store open.
fn deliver_in(
    ctx: &mut Ctx<'_>,
    store: &mut Store,
    own: &TeamName,
    message: &Message,
    team: &TeamName,
    all: &[&DeliveryRecord],
    tries: &[Try<'_>],
) -> Vec<Result<RecipientReport, SendError>> {
    let mut there = Ctx {
        home: ctx.home,
        store,
        registry: ctx.registry,
        platform: ctx.platform,
        clock: ctx.clock,
        config: ctx.config,
    };
    let copy = match receive(&mut there, own, message, all) {
        Ok(Some(copy)) => copy,
        Ok(None) => {
            return tries
                .iter()
                .map(|t| {
                    let outcome = held_by_closing(&there, team, message, t)?;
                    mirror(ctx, message, t, outcome)
                })
                .collect();
        }
        Err(e) => return tries.iter().map(|_| Err(e.clone())).collect(),
    };
    write_copy(&there, team, &copy.message);
    tries
        .iter()
        .map(|t| {
            let outcome = deliver_there(&mut there, team, &copy, t)?;
            mirror(ctx, message, t, outcome)
        })
        .collect()
}

/// Stores the recipient team's copy, or finds it from an earlier attempt.
/// `None` when that team is not open.
fn receive(
    there: &mut Ctx<'_>,
    own: &TeamName,
    message: &Message,
    all: &[&DeliveryRecord],
) -> Result<Option<StoredMessage>, SendError> {
    let received = ReceivedMessage {
        ulid: message.ulid,
        sender: message.sender.seen_from_outside(own),
        kind: message.kind,
        re: message.re,
        commit: message.commit.clone(),
        body: message.body.clone(),
        recipients: all
            .iter()
            .filter_map(|r| remote_of(&r.delivery.recipient).map(Remote::local))
            .collect(),
    };
    let now = there.clock.now();
    Ok(there.store.write(|tx| tx.insert_received(now, &received))?)
}

/// What became of one recipient in its own team.
struct Outcome {
    state: DeliveryState,
    route: Option<Route>,
    error: Option<DeliveryError>,
}

/// Types the line for one recipient in its own team, unless that team's row
/// says it already arrived. A row that an earlier attempt left behind gets
/// its attempt counted there first, as a resend there would.
fn deliver_there(
    there: &mut Ctx<'_>,
    team: &TeamName,
    copy: &StoredMessage,
    t: &Try<'_>,
) -> Result<Outcome, SendError> {
    let local = remote_of(&t.row.delivery.recipient)
        .map(Remote::local)
        .ok_or_else(|| SendError::Internal("a local row reached the remote path".to_owned()))?;
    let record = copy
        .deliveries
        .iter()
        .find(|d| d.delivery.recipient == local)
        .ok_or_else(|| {
            SendError::Internal(format!("the copy in team {team} has no row for {local}"))
        })?;
    let before = record.delivery.state;
    if matches!(before, DeliveryState::HandedOff | DeliveryState::Acked) {
        return Ok(outcome_of(record));
    }
    let attempts = if copy.duplicate {
        let now = there.clock.now();
        let ulid = copy.message.ulid;
        there
            .store
            .write(|tx| tx.begin_resend(now, &ulid, &local))?
    } else {
        record.delivery.attempts + 1
    };
    deliver_one(there, team, &copy.message, &local, before, attempts, None)?;
    let ulid = copy.message.ulid;
    let after = there
        .store
        .read(|tx| tx.delivery(&ulid, &local))?
        .ok_or_else(|| SendError::Internal(format!("the delivery to {local} vanished")))?;
    Ok(outcome_of(&after))
}

/// A read message was handed off as far as the sender can tell: `acked`
/// belongs to the reader's own team.
fn outcome_of(record: &DeliveryRecord) -> Outcome {
    let d = &record.delivery;
    let state = match d.state {
        DeliveryState::Acked => DeliveryState::HandedOff,
        other @ (DeliveryState::Pending
        | DeliveryState::HandedOff
        | DeliveryState::Unconfirmed
        | DeliveryState::Undelivered) => other,
    };
    Outcome {
        state,
        route: d.route,
        error: d.error.clone(),
    }
}

/// What a team that is no longer open already holds for one try. Nothing is
/// typed there, but a copy an earlier attempt stored still says what
/// happened: a row handed off or read arrived, and a row still `pending`
/// may have been typed before the sender died, so it is not `undelivered`.
fn held_by_closing(
    there: &Ctx<'_>,
    team: &TeamName,
    message: &Message,
    t: &Try<'_>,
) -> Result<Outcome, SendError> {
    let Some(local) = remote_of(&t.row.delivery.recipient).map(Remote::local) else {
        return Ok(gone(team));
    };
    let ulid = message.ulid;
    let Some(record) = there.store.read(|tx| tx.delivery(&ulid, &local))? else {
        return Ok(gone(team));
    };
    if record.delivery.state != DeliveryState::Pending {
        return Ok(outcome_of(&record));
    }
    let Outcome { error, .. } = gone(team);
    Ok(Outcome {
        state: DeliveryState::Unconfirmed,
        route: None,
        error,
    })
}

fn gone(team: &TeamName) -> Outcome {
    Outcome {
        state: DeliveryState::Undelivered,
        route: None,
        error: Some(DeliveryError::new(
            DeliveryErrorCode::TeamClosed,
            format!("team {team} is closing, closed or gone"),
        )),
    }
}

/// Records on the sender's row what happened in the recipient's team. The
/// same forward-only moves as a local delivery: a row another command
/// handed off stays so, and a copy that may have been typed never becomes
/// `undelivered` (INV-5). A row still `pending` there is left `pending` here.
fn mirror(
    ctx: &mut Ctx<'_>,
    message: &Message,
    t: &Try<'_>,
    outcome: Outcome,
) -> Result<RecipientReport, SendError> {
    let recipient = &t.row.delivery.recipient;
    let ulid = message.ulid;
    if outcome.state == DeliveryState::Pending {
        return Ok(RecipientReport {
            attempts: Some(t.attempts),
            ..report_of(t.row)
        });
    }
    let update = DeliveryUpdate {
        state: outcome.state,
        route: outcome.route,
        error: outcome.error,
        attempts: t.attempts,
    };
    let now = ctx.clock.now();
    let record = ctx.store.write(|tx| {
        let reader = tx.reader();
        let current = reader.delivery(&ulid, recipient)?;
        if let Some(c) = &current {
            let held = matches!(
                c.delivery.state,
                DeliveryState::Acked | DeliveryState::HandedOff
            ) || (c.delivery.state == DeliveryState::Unconfirmed
                && update.state == DeliveryState::Undelivered);
            if held {
                return Ok(current);
            }
        }
        tx.set_delivery_state(now, &ulid, recipient, &update)?;
        reader.delivery(&ulid, recipient)
    })?;
    record.map(|r| report_of(&r)).ok_or_else(|| {
        SendError::Internal(format!(
            "the delivery of {} to {recipient} vanished",
            message.display_id
        ))
    })
}

fn remote_of(recipient: &Recipient) -> Option<&Remote> {
    match recipient {
        Recipient::Other(remote) => Some(remote),
        Recipient::Lead | Recipient::Member(_) => None,
    }
}
