//! `send`: the outbox order of ADR 0003. Check, then store the message with
//! one `pending` delivery per recipient, then write the `comms/` copy, then
//! deliver to each recipient outside any transaction and record each outcome.

use std::path::PathBuf;

use serde_json::json;
use teamwire_core::{
    Actor, DeliveryError, DeliveryState, DisplayId, EventType, Message, MessageType, Pid,
    Recipient, Refusal, Route, SendRequest, TeamName, Timestamp, Ulid,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line, message_line};
use teamwire_store::{
    Ctx, DeliveryRecord, DeliveryUpdate, NewMessage, Party, StoreError, TeamRow, Tx,
};

use crate::{RecipientReport, SendError, SendReport, rules};

/// A message that is committed and about to be delivered. The CLI prints
/// it, id and key, before any host call, so a sender killed in the middle of
/// a delivery can still name its message (ADR 0003 point 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub id: Ulid,
    pub display_id: DisplayId,
    /// The sender's key, or the message's ULID when the sender gave none.
    pub key: String,
    /// True when the key already named a message: this is a resume.
    pub duplicate: bool,
}

/// The member's generation before a reprompt bumped it, kept to undo the
/// bump when the reprompt is known not to have arrived.
pub(crate) struct Bumped {
    recipient: Recipient,
    generation: u32,
    started_at: Timestamp,
}

/// What the storing transaction decided.
enum Stage {
    Stored {
        stored: Box<teamwire_store::StoredMessage>,
        bumped: Vec<Bumped>,
    },
    /// The team began closing between the check and the write (CLS-3).
    Closed,
}

/// Sends one message and delivers it. See `send_with`.
pub fn send(ctx: &mut Ctx<'_>, req: SendRequest) -> Result<SendReport, SendError> {
    send_with(ctx, req, &mut |_| {})
}

/// Sends one message. `on_stored` is called once the message is committed
/// and before the first host call.
///
/// A refusal records a `message.refused` event and stores nothing. A key
/// that already names a message from this sender resumes it: only the
/// recipients still `pending` or `undelivered` are tried again.
pub fn send_with(
    ctx: &mut Ctx<'_>,
    req: SendRequest,
    on_stored: &mut dyn FnMut(&Stored),
) -> Result<SendReport, SendError> {
    let checked = match rules::check(ctx, &req) {
        Ok(checked) => checked,
        Err(SendError::Refused(refusal)) => {
            record_refusal(ctx, Some(&req.actor), &refusal, Some(req.kind), &req.to);
            return Err(SendError::Refused(refusal));
        }
        Err(e) => return Err(e),
    };
    let SendRequest {
        actor,
        to,
        kind,
        body,
        client_key,
        ..
    } = req;
    let new = NewMessage {
        sender: actor.clone(),
        kind,
        re: checked.re,
        commit: checked.commit,
        body,
        client_key: client_key.clone(),
        recipients: checked.recipients,
    };
    let now = ctx.clock.now();
    let stage = ctx.store.write(|tx| store_message(tx, now, &new))?;
    let (stored, bumped) = match stage {
        Stage::Stored { stored, bumped } => (stored, bumped),
        Stage::Closed => {
            record_refusal(ctx, Some(&actor), &Refusal::TeamClosed, Some(kind), &to);
            return Err(Refusal::TeamClosed.into());
        }
    };
    let message = stored.message;
    on_stored(&Stored {
        id: message.ulid,
        display_id: message.display_id.clone(),
        key: client_key.unwrap_or_else(|| message.ulid.to_string()),
        duplicate: stored.duplicate,
    });
    let team = team_name(ctx)?;
    write_copy(ctx, &team, &message);

    let mut deliveries = Vec::new();
    for record in &stored.deliveries {
        let state = record.delivery.state;
        if !matches!(state, DeliveryState::Pending | DeliveryState::Undelivered) {
            deliveries.push(report_of(record));
            continue;
        }
        // A reprompt that was undone after a known failure starts its
        // generation again when the resume tries it again.
        let again = if stored.duplicate && state == DeliveryState::Undelivered {
            bump_again(ctx, &message, &record.delivery.recipient)?
        } else {
            None
        };
        let attempts = record.delivery.attempts + 1;
        let bump = bumped
            .iter()
            .find(|b| b.recipient == record.delivery.recipient)
            .or(again.as_ref());
        deliveries.push(deliver_one(
            ctx,
            &team,
            &message,
            &record.delivery.recipient,
            state,
            attempts,
            bump,
        )?);
    }
    Ok(SendReport {
        id: message.ulid,
        display_id: message.display_id,
        seq: message.seq,
        deliveries,
        duplicate_of_key: stored.duplicate,
    })
}

/// The one write transaction of a send: the message, its deliveries, its
/// event, and for a reprompt the members' next generations.
fn store_message(
    tx: &Tx<'_>,
    at: Timestamp,
    new: &NewMessage,
) -> Result<Stage, teamwire_store::StoreError> {
    let open = tx
        .reader()
        .team()?
        .is_some_and(|t| t.state == teamwire_core::TeamState::Open);
    if !open {
        return Ok(Stage::Closed);
    }
    let stored = tx.insert_message(at, new)?;
    let mut bumped = Vec::new();
    if new.kind == MessageType::Reprompt && !stored.duplicate {
        for recipient in &new.recipients {
            if let Some(b) = bump(tx, at, &stored.message.display_id, recipient)? {
                bumped.push(b);
            }
        }
    }
    Ok(Stage::Stored {
        stored: Box::new(stored),
        bumped,
    })
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
    let line = message_line(
        team,
        &message.display_id,
        message.kind,
        &message.sender,
        &message.body,
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

/// The team row.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub(crate) fn read_team(ctx: &Ctx<'_>) -> Result<Option<TeamRow>, StoreError> {
    ctx.store.read(|tx| tx.team())
}

pub(crate) fn team_name(ctx: &Ctx<'_>) -> Result<TeamName, SendError> {
    let team = read_team(ctx)?;
    team.map(|t| t.name)
        .ok_or_else(|| SendError::Internal("the store has no team row".to_owned()))
}

/// Writes `comms/<display-id>.md`. The store holds the original, so a
/// failure here is reported in the log and left for `verify --repair-files`.
fn write_copy(ctx: &Ctx<'_>, team: &TeamName, message: &Message) {
    let dir: PathBuf = ctx.home.join("teams").join(team.as_str()).join("comms");
    let path = dir.join(format!("{}.md", message.display_id));
    let written = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            ctx.platform
                .write_atomic(&path, message.body.as_bytes())
                .map_err(|e| e.to_string())
        });
    if let Err(e) = written {
        tracing::warn!(path = %path.display(), error = %e, "could not write the comms copy");
    }
}

/// Records a refused send as a `message.refused` event and nothing else
/// (MSG-15). The text of the message is not recorded. A failure to record is
/// logged: the refusal still reaches the sender.
pub fn record_refusal(
    ctx: &mut Ctx<'_>,
    actor: Option<&Actor>,
    refusal: &Refusal,
    kind: Option<MessageType>,
    to: &[Recipient],
) {
    let now = ctx.clock.now();
    let role = match actor {
        Some(Actor::Member(role)) => Some(role.clone()),
        _ => None,
    };
    let data = json!({
        "code": refusal.code(),
        "type": kind.map(MessageType::as_str),
        "sender": actor.map(Actor::as_str),
        "to": to.iter().map(Recipient::as_str).collect::<Vec<_>>(),
    });
    let written = ctx
        .store
        .write(|tx| tx.record_event(now, EventType::MessageRefused, role.as_ref(), None, &data));
    if let Err(e) = written {
        tracing::warn!(error = %e, "could not record a refused send");
    }
}

/// The pid of the live watch on this team, if one holds the lease (`send`
/// tells the lead after a reprompt whether anything is watching).
pub fn watch_running(ctx: &Ctx<'_>) -> Result<Option<Pid>, SendError> {
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::watch_lease` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    let lease = ctx.store.read(|tx| tx.watch_lease())?;
    Ok(lease
        .map(|l| l.pid)
        .filter(|pid| ctx.platform.pid_alive(*pid)))
}
