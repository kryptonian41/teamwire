//! `send`: the outbox order of ADR 0003. Check, then store the message with
//! one `pending` delivery per recipient, then write the `comms/` copy, then
//! deliver to each recipient outside any transaction and record each outcome.

use std::path::PathBuf;

use serde_json::json;
use teamwire_core::{
    Actor, DeliveryErrorCode, DeliveryState, DisplayId, EventType, Message, MessageType, Pid,
    Recipient, Refusal, SendRequest, TeamName, Timestamp, Ulid,
};
use teamwire_deliver::watch_lease_is_dead;
use teamwire_store::{Ctx, DeliveryRecord, NewMessage, StoreError, TeamRow, Tx};

use crate::attempt::{
    Bumped, Waking, bump, bump_again, busy, claim_attempt, deliver_one, earlier_bump, hold,
    is_held, report_of, waking,
};
use crate::remote::{Try, deliver_to_team};
use crate::{RecipientReport, RecipientState, SendError, SendReport, rules};

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

/// `send_with`, and `also` runs inside the transaction that stores the
/// message, so both are saved or neither is. `teamwire amend` changes the
/// member's scope this way (ADR 0046): a scope change without its note would
/// let `close --role` accept a pass made with the old check. An error from
/// `also` rolls the message back and is returned. A closed team runs no
/// `also`, and a resume of a stored key does not run it again.
pub fn send_with_write(
    ctx: &mut Ctx<'_>,
    mut req: SendRequest,
    also: &dyn Fn(&Tx<'_>) -> Result<(), StoreError>,
    on_stored: &mut dyn FnMut(&Stored),
) -> Result<SendReport, SendError> {
    // A file written on Windows has a byte-order mark and CRLF line ends,
    // which are not text the sender chose. Checks, the stored copy and its
    // hash all see the plain body.
    req.body = normalise_body(&req.body);
    let own = team_name(ctx)?;
    req.to = local_names(std::mem::take(&mut req.to), &own);
    let checked = match rules::check(ctx, &req) {
        Ok(checked) => checked,
        Err(SendError::Refused(refusal)) => {
            record_refusal(ctx, Some(&req.actor), &refusal, Some(req.kind), &req.to);
            return Err(SendError::Refused(refusal));
        }
        Err(SendError::InvalidArgument(why)) => {
            record_invalid(ctx, &req.actor, Some(req.kind), &req.to);
            return Err(SendError::InvalidArgument(why));
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
    // The time is read under the store lock, so the order of the stored
    // times is the order of the commits. `close --role` compares an amend
    // note's time with verify records' times (ADR 0046): a time read before
    // the lock could be later than that of a write that committed first.
    let stage = ctx
        .store
        .write(|tx| store_message(tx, ctx.clock.now(), &new, also))?;
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
    let team = own;
    write_copy(ctx, &team, &message);

    // A held row waits for the handoff or a reprompt, also on a resume.
    let to_try = |r: &DeliveryRecord| {
        matches!(
            r.delivery.state,
            DeliveryState::Pending | DeliveryState::Undelivered
        ) && !is_held(r)
    };
    let mut deliveries = Vec::new();
    for record in &stored.deliveries {
        if !to_try(record) {
            deliveries.push(report_of(record));
            continue;
        }
        if record.delivery.recipient.other_team().is_some() {
            continue;
        }
        let attempts = record.delivery.attempts + 1;
        let result = try_recipient(
            ctx,
            &actor,
            &team,
            &message,
            record,
            stored.duplicate,
            &bumped,
        );
        deliveries.push(survive_error(result, record, attempts)?);
    }
    for (other, rows) in by_other_team(&stored.deliveries) {
        let mut tries: Vec<Try<'_>> = Vec::new();
        for row in rows.iter().copied().filter(|r| to_try(r)) {
            if !stored.duplicate {
                tries.push(Try {
                    row,
                    attempts: row.delivery.attempts + 1,
                });
                continue;
            }
            match claim_attempt(ctx, &actor, &message, row) {
                Ok(Some(attempts)) => tries.push(Try { row, attempts }),
                Ok(None) => deliveries.push(busy(row)),
                Err(e) => deliveries.push(survive_error(Err(e), row, row.delivery.attempts + 1)?),
            }
        }
        if tries.is_empty() {
            continue;
        }
        let results = deliver_to_team(ctx, &team, &message, &other, &rows, &tries, "send");
        for (t, result) in tries.iter().zip(results) {
            deliveries.push(survive_error(result, t.row, t.attempts)?);
        }
    }
    Ok(SendReport {
        id: message.ulid,
        display_id: message.display_id,
        seq: message.seq,
        deliveries,
        duplicate_of_key: stored.duplicate,
    })
}

/// Sends one message. `on_stored` is called once the message is committed
/// and before the first host call.
///
/// A refusal records a `message.refused` event and stores nothing. A key
/// that already names a message from this sender resumes it: only the
/// recipients still `pending` or `undelivered` are tried again, and a
/// `pending` one only once its latest attempt began over a minute ago, as
/// for `resend`; a younger one is `skipped`.
pub fn send_with(
    ctx: &mut Ctx<'_>,
    req: SendRequest,
    on_stored: &mut dyn FnMut(&Stored),
) -> Result<SendReport, SendError> {
    send_with_write(ctx, req, &|_| Ok(()), on_stored)
}

/// Each recipient once, and a name that points back at the sender's own
/// team (`api@<own>`) as its local name, so the rules and the rows see one
/// spelling for one party.
fn local_names(to: Vec<Recipient>, own: &TeamName) -> Vec<Recipient> {
    let mut out: Vec<Recipient> = Vec::with_capacity(to.len());
    for r in to {
        let r = r.local_to(own);
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// The rows that name agents of other teams, grouped by team in the order
/// the teams first appear.
pub(crate) fn by_other_team(rows: &[DeliveryRecord]) -> Vec<(TeamName, Vec<&DeliveryRecord>)> {
    let mut groups: Vec<(TeamName, Vec<&DeliveryRecord>)> = Vec::new();
    for row in rows {
        let Some(team) = row.delivery.recipient.other_team() else {
            continue;
        };
        match groups.iter_mut().find(|(t, _)| t == team) {
            Some((_, group)) => group.push(row),
            None => groups.push((team.clone(), vec![row])),
        }
    }
    groups
}

/// The body without a leading byte-order mark and with `\r\n` as `\n`.
fn normalise_body(body: &str) -> String {
    body.strip_prefix('\u{feff}')
        .unwrap_or(body)
        .replace("\r\n", "\n")
}

/// Delivers to one recipient of a stored message. A resume first claims
/// the row, as a resend does, so a row another command is still delivering
/// is `skipped` and not typed a second time.
fn try_recipient(
    ctx: &mut Ctx<'_>,
    actor: &Actor,
    team: &TeamName,
    message: &Message,
    record: &DeliveryRecord,
    duplicate: bool,
    bumped: &[Bumped],
) -> Result<RecipientReport, SendError> {
    let state = record.delivery.state;
    let attempts = if duplicate {
        let Some(attempts) = claim_attempt(ctx, actor, message, record)? else {
            return Ok(busy(record));
        };
        attempts
    } else {
        record.delivery.attempts + 1
    };
    // A reprompt that was undone after a known failure starts its
    // generation again when the resume tries it again.
    let again = if duplicate && state == DeliveryState::Undelivered {
        bump_again(ctx, message, &record.delivery.recipient)?
    } else if duplicate {
        // Still pending: an earlier run bumped the generation and died
        // before it could say whether the line arrived.
        earlier_bump(ctx, message, &record.delivery.recipient)?
    } else {
        None
    };
    let bump = bumped
        .iter()
        .find(|b| b.recipient == record.delivery.recipient)
        .or(again.as_ref());
    deliver_one(
        ctx,
        team,
        message,
        &record.delivery.recipient,
        state,
        attempts,
        bump,
    )
}

/// Turns an error that belongs to one recipient into that recipient's
/// report, so the recipients after it are still tried and the caller still
/// gets every report of a send that already typed some lines. A store error
/// is `pending`: the delivery may or may not have been typed, and the
/// command exits 6 for it. A team that started closing part way types
/// nothing more and counts no attempt. The row is then `unconfirmed
/// (team_closed)` when an earlier attempt may have typed it (`pending` or
/// `unconfirmed`), else `undelivered (team_closed)`, which exits 4. Any
/// other error ends the whole command.
pub(crate) fn survive_error(
    result: Result<RecipientReport, SendError>,
    row: &DeliveryRecord,
    attempts: u32,
) -> Result<RecipientReport, SendError> {
    let d = &row.delivery;
    let (state, reason, attempts) = match result {
        Err(SendError::Store(e)) => {
            tracing::warn!(recipient = %d.recipient, error = %e, "could not record a delivery");
            (RecipientState::Pending, None, attempts)
        }
        Err(SendError::Refused(Refusal::TeamClosed)) => {
            let state = match d.state {
                DeliveryState::Pending | DeliveryState::Unconfirmed => RecipientState::Unconfirmed,
                DeliveryState::Undelivered | DeliveryState::HandedOff | DeliveryState::Acked => {
                    RecipientState::Undelivered
                }
            };
            (state, Some(DeliveryErrorCode::TeamClosed), d.attempts)
        }
        other => return other,
    };
    Ok(RecipientReport {
        to: d.recipient.clone(),
        state,
        reason,
        attempts: Some(attempts),
        busy: false,
    })
}

/// The one write transaction of a send: the message, its deliveries, its
/// event, and for a reprompt the members' next generations.
fn store_message(
    tx: &Tx<'_>,
    at: Timestamp,
    new: &NewMessage,
    also: &dyn Fn(&Tx<'_>) -> Result<(), StoreError>,
) -> Result<Stage, teamwire_store::StoreError> {
    let open = tx
        .reader()
        .team()?
        .is_some_and(|t| t.state == teamwire_core::TeamState::Open);
    if !open {
        return Ok(Stage::Closed);
    }
    let stored = tx.insert_message(at, new)?;
    if !stored.duplicate {
        // A resume stored nothing new, so the caller's write already ran
        // with the first send.
        also(tx)?;
    }
    if new.kind == MessageType::Answer && !stored.duplicate {
        settle_question(tx, at, new)?;
    }
    let mut bumped = Vec::new();
    let mut held = Vec::new();
    if !stored.duplicate {
        for recipient in &new.recipients {
            // A reprompt starts the next generation. Any other message to a
            // member whose newest result is in its current generation, and
            // whose turn has ended, wakes it, and that turn is a generation
            // too (ADR 0043).
            // A member whose newest result asked for a handoff gets the
            // message held for the fresh session instead (decision 11).
            let kind = if new.kind == MessageType::Reprompt {
                EventType::MemberReprompted
            } else {
                match waking(tx, recipient)? {
                    Waking::Wake => EventType::MemberWoken,
                    Waking::Hold => {
                        hold(tx, at, &stored.message.ulid, recipient)?;
                        held.push(recipient.clone());
                        continue;
                    }
                    Waking::Nothing => continue,
                }
            };
            if let Some(b) = bump(tx, at, &stored.message.display_id, recipient, kind)? {
                bumped.push(b);
            }
        }
    }
    let mut stored = stored;
    if !held.is_empty() {
        // The rows read back as written, so the report says `held`.
        stored.deliveries = tx
            .reader()
            .deliveries_of(&stored.message.ulid)?
            .all_or_corrupt()?;
    }
    Ok(Stage::Stored {
        stored: Box::new(stored),
        bumped,
    })
}

/// An answer settles the delivery of its question to the answerer (ADR
/// 0043 decision 3): the lead that answers a question it read from its file
/// is no longer told about it. A `re` with no delivery to the sender, such
/// as a message that was not a question to it, changes nothing.
fn settle_question(
    tx: &Tx<'_>,
    at: Timestamp,
    answer: &NewMessage,
) -> Result<(), teamwire_store::StoreError> {
    let Some(question) = &answer.re else {
        return Ok(());
    };
    let answerer = match &answer.sender {
        Actor::Lead => Recipient::Lead,
        Actor::Member(role) => Recipient::Member(role.clone()),
        Actor::Other(remote) => Recipient::Other(remote.clone()),
        Actor::User => return Ok(()),
    };
    if tx.reader().delivery(question, &answerer)?.is_some() {
        tx.ack_delivery(at, question, &answerer)?;
    }
    Ok(())
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
pub(crate) fn write_copy(ctx: &Ctx<'_>, team: &TeamName, message: &Message) {
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
        "reason": refusal.code(),
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

/// Records a send by a proven sender that was malformed, such as one with
/// no recipient, as `message.refused` with the code `invalid_argument`. It
/// stores nothing, as a refusal does; the record lets `teamwire close` tell
/// that the lead's newest send never went out (ADR 0057).
pub fn record_invalid(
    ctx: &mut Ctx<'_>,
    actor: &Actor,
    kind: Option<MessageType>,
    to: &[Recipient],
) {
    let now = ctx.clock.now();
    let role = match actor {
        Actor::Member(role) => Some(role.clone()),
        Actor::Lead | Actor::User | Actor::Other(_) => None,
    };
    let data = json!({
        "code": "invalid_argument",
        "reason": "invalid_argument",
        "type": kind.map(MessageType::as_str),
        "sender": actor.as_str(),
        "to": to.iter().map(Recipient::as_str).collect::<Vec<_>>(),
    });
    let written = ctx
        .store
        .write(|tx| tx.record_event(now, EventType::MessageRefused, role.as_ref(), None, &data));
    if let Err(e) = written {
        tracing::warn!(error = %e, "could not record a malformed send");
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
    let now = ctx.clock.now();
    Ok(lease
        .filter(|l| !watch_lease_is_dead(l, ctx.platform, now))
        .map(|l| l.pid))
}

/// `prove_actor`, and a refusal is recorded as `message.refused` (MSG-15).
/// Every front end (the CLI, the MCP server) calls this one, so a refused
/// identity is logged the same way whoever asked.
pub fn prove_actor_logged(ctx: &mut Ctx<'_>, env: &crate::CallerEnv) -> Result<Actor, Refusal> {
    crate::prove_actor(ctx, env).inspect_err(|refusal| {
        record_refusal(ctx, None, refusal, None, &[]);
    })
}

/// Reads a message type from text. A type outside the closed set is
/// `type_not_allowed`, recorded as `message.refused` for the proven sender.
pub fn parse_type_logged(
    ctx: &mut Ctx<'_>,
    actor: &Actor,
    text: &str,
    to: &[Recipient],
) -> Result<MessageType, Refusal> {
    MessageType::parse(text).map_err(|_| {
        let refusal = Refusal::TypeNotAllowed;
        record_refusal(ctx, Some(actor), &refusal, None, to);
        refusal
    })
}
