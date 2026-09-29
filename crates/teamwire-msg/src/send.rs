//! `send`: the outbox order of ADR 0003. Check, then store the message with
//! one `pending` delivery per recipient, then write the `comms/` copy, then
//! deliver to each recipient outside any transaction and record each outcome.

use std::path::PathBuf;

use serde_json::json;
use teamwire_core::{
    Actor, DeliveryState, DisplayId, EventType, Message, MessageType, Pid, Recipient, Refusal,
    SendRequest, TeamName, Timestamp, Ulid,
};
use teamwire_store::{Ctx, NewMessage, StoreError, TeamRow, Tx};

use crate::attempt::{Bumped, bump, bump_again, deliver_one, report_of};
use crate::{SendError, SendReport, rules};

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
