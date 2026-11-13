//! One delivery attempt to one recipient: locate and type outside any
//! transaction, record the outcome in a short one, and keep a reprompt's
//! generation in step with what is known about its delivery.

use serde_json::json;
use teamwire_core::{
    Actor, DeliveryError, DeliveryErrorCode, DeliveryState, DisplayId, EventType, MemberState,
    Message, MessageType, Recipient, Refusal, Route, TeamName, TeamState, Timestamp,
};
use teamwire_deliver::{AttemptHistory, CommsCopy, DeliveryOutcome, deliver_line};
use teamwire_store::{Ctx, DeliveryRecord, DeliveryUpdate, Party, Tx};

use crate::send::record_refusal;
use crate::{RecipientReport, RecipientState, SendError};

/// Claims one more attempt at `row`, which the caller read earlier, for a
/// resend or a send resumed by its key. `None` when another command is on
/// the row: the row moved since the caller read it, or its latest attempt
/// began under `PENDING_STALE` ago with no outcome recorded since, which is
/// a command still delivering it. The checks, the count and its
/// `delivery.resent` are one write, so of two commands claiming one row
/// only the first types; the second sees the fresh attempt. A team that is
/// not open refuses `team_closed`, recorded as `message.refused` (CLS-3).
pub(crate) fn claim_attempt(
    ctx: &mut Ctx<'_>,
    actor: &Actor,
    message: &Message,
    row: &DeliveryRecord,
) -> Result<Option<u32>, SendError> {
    enum Claim {
        Counted(u32),
        Busy,
        Closed,
    }
    let recipient = &row.delivery.recipient;
    let now = ctx.clock.now();
    let claim = ctx.store.write(|tx| {
        let reader = tx.reader();
        if reader.team()?.is_none_or(|t| t.state != TeamState::Open) {
            return Ok(Claim::Closed);
        }
        let moved = reader.delivery(&message.ulid, recipient)?.is_none_or(|c| {
            c.delivery.state != row.delivery.state || c.delivery.attempts != row.delivery.attempts
        });
        if moved || AttemptHistory::read(&reader, message, recipient)?.in_flight(now) {
            return Ok(Claim::Busy);
        }
        tx.begin_resend(now, &message.ulid, recipient)
            .map(Claim::Counted)
    })?;
    match claim {
        Claim::Counted(attempts) => Ok(Some(attempts)),
        Claim::Busy => Ok(None),
        Claim::Closed => {
            record_refusal(
                ctx,
                Some(actor),
                &Refusal::TeamClosed,
                Some(message.kind),
                std::slice::from_ref(recipient),
            );
            Err(Refusal::TeamClosed.into())
        }
    }
}

/// The report for a row this command left to another.
pub(crate) fn skipped(row: &DeliveryRecord) -> RecipientReport {
    RecipientReport {
        state: RecipientState::Skipped,
        ..report_of(row)
    }
}

/// The member's generation before a reprompt, or a waking message, bumped
/// it, kept to undo the bump when the message is known not to have arrived.
/// `kind` is the event the bump wrote, `member.reprompted` or
/// `member.woken`, and the undo writes the same.
pub(crate) struct Bumped {
    pub(crate) recipient: Recipient,
    generation: u32,
    started_at: Timestamp,
    kind: EventType,
}

/// Starts the recipient's next generation and remembers the one it replaced.
/// `kind` is `member.reprompted` for a reprompt and `member.woken` for a
/// message that wakes a finished member (ADR 0043).
pub(crate) fn bump(
    tx: &Tx<'_>,
    at: Timestamp,
    id: &DisplayId,
    recipient: &Recipient,
    kind: EventType,
) -> Result<Option<Bumped>, teamwire_store::StoreError> {
    let Recipient::Member(role) = recipient else {
        return Ok(None);
    };
    let Some(member) = tx.reader().member(role)? else {
        return Ok(None);
    };
    if kind == EventType::MemberWoken {
        tx.wake_generation(at, role, id.as_str())?;
    } else {
        tx.bump_generation(at, role, id.as_str())?;
    }
    Ok(Some(Bumped {
        recipient: recipient.clone(),
        generation: member.generation,
        started_at: member.gen_started_at,
        kind,
    }))
}

/// What a message that is not a reprompt does to a member recipient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waking {
    /// The member works on its current generation, or is no started member.
    Nothing,
    /// The member's newest result is in its current generation: the message
    /// starts its next one, which the watch tracks (ADR 0043 decision 8).
    Wake,
    /// That newest result asked for a handoff: the message waits, untyped,
    /// for the fresh session or a reprompt (ADR 0043 decision 11).
    Hold,
}

/// The summary prefix of a result that asks the lead for a handoff.
const HANDOFF_PREFIX: &str = "handoff:";

/// Whether a message that is not a reprompt wakes `recipient`: a started
/// member whose newest completion, of any status, is in its current
/// generation. A completion whose summary starts with `handoff:` holds the
/// message instead.
pub(crate) fn waking(
    tx: &Tx<'_>,
    recipient: &Recipient,
) -> Result<Waking, teamwire_store::StoreError> {
    let Recipient::Member(role) = recipient else {
        return Ok(Waking::Nothing);
    };
    let Some(member) = tx.reader().member(role)? else {
        return Ok(Waking::Nothing);
    };
    if member.state != MemberState::Started {
        return Ok(Waking::Nothing);
    }
    let newest = tx
        .reader()
        .latest_completion(role)?
        .filter(|c| c.completion.generation == member.generation);
    Ok(match newest {
        None => Waking::Nothing,
        Some(c) if c.completion.summary.starts_with(HANDOFF_PREFIX) => Waking::Hold,
        Some(_) => Waking::Wake,
    })
}

/// The `undelivered` row of a message held for a handoff.
pub(crate) fn is_held(row: &DeliveryRecord) -> bool {
    row.delivery.state == DeliveryState::Undelivered
        && row
            .delivery
            .error
            .as_ref()
            .is_some_and(|e| e.code == DeliveryErrorCode::HeldForHandoff)
}

/// Writes `recipient`'s new row as held: `undelivered` with
/// `held_for_handoff`, no attempt and nothing typed. `inbox` still lists it,
/// so the fresh session reads it, and a reprompt names it.
pub(crate) fn hold(
    tx: &Tx<'_>,
    at: Timestamp,
    message: &teamwire_core::Ulid,
    recipient: &Recipient,
) -> Result<(), teamwire_store::StoreError> {
    tx.set_delivery_state(
        at,
        message,
        recipient,
        &DeliveryUpdate {
            state: DeliveryState::Undelivered,
            route: None,
            error: Some(DeliveryError::new(
                DeliveryErrorCode::HeldForHandoff,
                "the member's newest result asked for a handoff",
            )),
            attempts: 0,
        },
    )
}

/// The bump an earlier, interrupted attempt of this reprompt left behind: the
/// row is still `pending`, so no failure of that attempt was ever undone. The
/// member's own event history says which generation and start time the bump
/// replaced. `None` when this reprompt's bump is not the member's latest, so
/// that giving it back would undo a later one.
pub(crate) fn earlier_bump(
    ctx: &Ctx<'_>,
    message: &Message,
    recipient: &Recipient,
) -> Result<Option<Bumped>, SendError> {
    let Recipient::Member(role) = recipient else {
        return Ok(None);
    };
    let (events, member) = ctx.store.read(|tx| {
        let kinds = [
            EventType::MemberCreating,
            EventType::MemberReprompted,
            EventType::MemberWoken,
            EventType::MemberHandoff,
        ];
        Ok((tx.events_of_kinds(role, &kinds)?, tx.member(role)?))
    })?;
    let Some(member) = member else {
        return Ok(None);
    };
    // Each generation with the time it started, the message that started it
    // and the event that recorded it. A `member.creating` event starts
    // generation 1, a bump (a reprompt or a wake, ADR 0043) or a handoff
    // pushes the next one, and an undo pops a bump again. A handoff has no
    // undo and no message. A message that bumped nothing, such as a note to
    // a working member, never matches the newest start, so it gives none
    // back.
    let mut starts: Vec<(u32, Timestamp, Option<String>, EventType)> = Vec::new();
    for e in &events {
        let generation = e
            .data
            .get("generation")
            .and_then(serde_json::Value::as_u64)
            .and_then(|g| u32::try_from(g).ok());
        if e.kind == EventType::MemberCreating {
            starts = vec![(1, e.time, None, e.kind)];
        } else if e.kind == EventType::MemberHandoff {
            if let Some(g) = generation {
                starts.push((g, e.time, None, e.kind));
            }
        } else if matches!(e.kind, EventType::MemberReprompted | EventType::MemberWoken) {
            if e.data.get("undone") == Some(&json!(true)) {
                if starts.len() > 1 {
                    starts.pop();
                }
            } else if let Some(g) = generation {
                let by = e
                    .data
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                starts.push((g, e.time, by, e.kind));
            }
        }
    }
    let [.., (before, started_at, _, _), (now, _, by, kind)] = starts.as_slice() else {
        return Ok(None);
    };
    if by.as_deref() != Some(message.display_id.as_str()) || *now != member.generation {
        return Ok(None);
    }
    Ok(Some(Bumped {
        recipient: recipient.clone(),
        generation: *before,
        started_at: *started_at,
        kind: *kind,
    }))
}

/// A resume of a reprompt whose earlier failure was undone: bump again, in a
/// transaction that first checks the team is still open. Another message
/// bumps again only when it wakes the member now, as `send` decides
/// (ADR 0043).
pub(crate) fn bump_again(
    ctx: &mut Ctx<'_>,
    message: &Message,
    recipient: &Recipient,
) -> Result<Option<Bumped>, SendError> {
    let now = ctx.clock.now();
    let id = message.display_id.clone();
    if message.kind != MessageType::Reprompt {
        // A wake is a side effect of the message, never a reason to refuse
        // it, so a team that is closing just gets no new generation.
        return Ok(ctx.store.write(|tx| {
            let open = tx
                .reader()
                .team()?
                .is_some_and(|t| t.state == teamwire_core::TeamState::Open);
            if !open || waking(tx, recipient)? != Waking::Wake {
                return Ok(None);
            }
            bump(tx, now, &id, recipient, EventType::MemberWoken)
        })?);
    }
    let outcome = ctx.store.write(|tx| {
        let open = tx
            .reader()
            .team()?
            .is_some_and(|t| t.state == teamwire_core::TeamState::Open);
        if !open {
            return Ok(None);
        }
        bump(tx, now, &id, recipient, EventType::MemberReprompted).map(Some)
    })?;
    match outcome {
        None => Err(Refusal::TeamClosed.into()),
        Some(bumped) => Ok(bumped),
    }
}

/// The most tries one command adds because the recipient's turn ended
/// during its attempt (ADR 0028). A recipient that stops during each try was
/// held for one more turn each time, and its next stop names the message.
const MAX_STOP_RETRIES: usize = 3;

/// What the outcome transaction did: recorded the row, or found that the
/// recipient stopped since the last look and left it for another try.
enum Written {
    Recorded(Option<DeliveryRecord>),
    TryAgain(usize),
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
    // A failed write leaves the copy missing or holding an older text, and
    // the lead must not read either as this message.
    let comms_written = *recipient != Recipient::Lead
        || std::fs::read(&comms_path).is_ok_and(|b| b == message.body.as_bytes());
    let comms = CommsCopy {
        path: &comms_path,
        written: comms_written,
    };
    let line = crate::lead_copy::line_for(ctx, team, message, recipient, comms)?;
    let party = match recipient {
        Recipient::Lead => Party::Lead,
        Recipient::Member(role) => Party::Member(role.clone()),
        // Typed through that team's own store, by `remote`.
        Recipient::Other(_) => {
            return Err(SendError::Internal(format!(
                "{recipient} is delivered through its own team"
            )));
        }
    };
    let mut stops_seen = 0;
    let mut stop_retries = 0;
    let record = loop {
        let outcome = deliver_line(ctx, &party, &line);
        let (state, route, error) = forward_only(before, outcome);
        let now = ctx.clock.now();
        let update = DeliveryUpdate {
            state,
            route,
            error,
            attempts,
        };
        let written = ctx.store.write(|tx| {
            let reader = tx.reader();
            let current = reader.delivery(&message.ulid, recipient)?;
            // The recipient may have read it, or another command handed it
            // off, while this one was typing. Those states are final here. So
            // is `unconfirmed` against a clean failure: the other command may
            // have typed the line, and the store refuses that move (INV-5,
            // REP-3).
            if let Some(c) = &current
                && (matches!(
                    c.delivery.state,
                    DeliveryState::Acked | DeliveryState::HandedOff
                ) || (c.delivery.state == DeliveryState::Unconfirmed
                    && update.state == DeliveryState::Undelivered))
            {
                return Ok(Written::Recorded(current));
            }
            // The recipient's turn ended while this row was `pending`, so its
            // hook left the message to this command, and no later stop would
            // name it once it is `undelivered` (ADR 0028). The check and the
            // write are one transaction, as the hook's list and mark are.
            if update.state == DeliveryState::Undelivered && stop_retries < MAX_STOP_RETRIES {
                let stops = AttemptHistory::read(&reader, message, recipient)
                    .map(|a| a.stops())
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "could not read whether the recipient stopped");
                        0
                    });
                if stops > stops_seen {
                    return Ok(Written::TryAgain(stops));
                }
            }
            tx.set_delivery_state(now, &message.ulid, recipient, &update)?;
            reader
                .delivery(&message.ulid, recipient)
                .map(Written::Recorded)
        })?;
        match written {
            Written::Recorded(record) => break record,
            Written::TryAgain(stops) => {
                stops_seen = stops;
                stop_retries += 1;
            }
        }
    };
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
    let undone = ctx.store.write(|tx| {
        tx.restore_generation(now, role, bumped.generation, bumped.started_at, bumped.kind)
    });
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
