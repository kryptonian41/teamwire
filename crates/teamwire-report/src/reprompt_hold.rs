//! The one corrective hold of a generation that a reprompt started (ADR
//! 0057). The ordinary hold names a message once (ADR 0003 point 8), and the
//! tool hook's reminder uses that once up: a member that was reminded after a
//! tool call, then answered in chat and stopped, was never held. A reminder
//! is not reading, and an answer in chat is not a result. So at a stop the
//! reprompt that started the member's generation is checked once more:
//!
//! - still unread, and too long for its typed line: hold, naming it;
//! - read, but the generation has no result: hold, asking for one.
//!
//! At most one such hold per generation, recorded on its `member.held`
//! event as `corrective`, so a member that still does not finish stays
//! unresolved and visible to the lead, and never loops. A hold never records
//! a result: only the member's own `done` does.

use serde_json::{Value, json};
use teamwire_core::{
    Actor, DeliveryErrorCode, DeliveryState, EventType, Member, MessageType, Recipient, TeamState,
};
use teamwire_deliver::line_holds_whole;
use teamwire_store::{CompletionRecord, Ctx, ReadTx, StoreError};

/// Why a generation's reprompt holds the member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The member has not read it, and its typed line did not carry it.
    Unread,
    /// The member read it and recorded no result in the generation.
    NoResult,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::NoResult => "no_result",
        }
    }
}

/// A corrective hold the stop owes the member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Corrective {
    pub(crate) display_id: String,
    pub(crate) generation: u32,
    pub(crate) from_user: bool,
    pub(crate) reason: Reason,
}

impl Corrective {
    /// The value recorded under `corrective` on the hold's event.
    pub(crate) fn data(&self) -> Value {
        json!({
            "message": self.display_id,
            "generation": self.generation,
            "reason": self.reason.as_str(),
        })
    }

    /// What the member is told. It names the message and the facts, and the
    /// tool and command that read it.
    pub(crate) fn note(&self) -> String {
        let from = if self.from_user {
            "the human user"
        } else {
            "the lead"
        };
        let id = &self.display_id;
        match self.reason {
            Reason::Unread => format!(
                "Reprompt {id} from {from} is still unread: a reminder after a tool call is not \
                 reading it, and its typed line held only a pointer. Read its full text with the \
                 team_inbox tool (mcp__teamwire__team_inbox) or the inbox command your contract \
                 names, do what it asks, update your report, and record done, blocked or failed \
                 for it."
            ),
            Reason::NoResult => format!(
                "Reprompt {id} from {from} started a new generation, and it has no result yet: \
                 an answer in chat or a finding is not a result. Update your report and record \
                 done, blocked or failed with team_done (mcp__teamwire__team_done) or the done \
                 command your contract names; if you cannot go on, ask the lead with a question."
            ),
        }
    }
}

/// The corrective hold this stop owes `member`, if any. `latest` is the
/// member's newest completion when it is in the current generation.
///
/// None when: the team is not open (a close is under way); the generation
/// was not started by a reprompt from the lead or the user (a wake, a
/// handoff, launch, or an undone reprompt); this generation already had its
/// corrective hold; the reprompt is held for a handoff or still being sent;
/// the member waits for an answer to a question it asked in the generation;
/// or the reprompt is read and the generation has a result.
pub(crate) fn owed(
    ctx: &Ctx<'_>,
    member: &Member,
    latest: Option<&CompletionRecord>,
) -> Result<Option<Corrective>, StoreError> {
    ctx.store.read(|tx| owed_in(tx, member, latest))
}

fn owed_in(
    tx: &ReadTx<'_>,
    member: &Member,
    latest: Option<&CompletionRecord>,
) -> Result<Option<Corrective>, StoreError> {
    if tx.team()?.is_none_or(|t| t.state != TeamState::Open) {
        return Ok(None);
    }
    let role = &member.role;
    let started_by = tx.latest_of_kinds(
        Some(role),
        &[
            EventType::MemberReprompted,
            EventType::MemberWoken,
            EventType::MemberHandoff,
        ],
    )?;
    let Some(start) = started_by.filter(|e| {
        e.kind == EventType::MemberReprompted
            && e.data.get("generation").and_then(Value::as_u64)
                == Some(u64::from(member.generation))
    }) else {
        return Ok(None);
    };
    let Some(id) = start.data.get("message").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Ok(display) = teamwire_core::DisplayId::parse(id) else {
        return Ok(None);
    };
    let Some(message) = tx.message_by_display_id(&display)? else {
        return Ok(None);
    };
    if message.kind != MessageType::Reprompt || !matches!(message.sender, Actor::Lead | Actor::User)
    {
        return Ok(None);
    }
    if already_held(tx, member)? {
        return Ok(None);
    }
    let recipient = Recipient::Member(role.clone());
    let Some(delivery) = tx.delivery(&message.ulid, &recipient)? else {
        return Ok(None);
    };
    let held_for_handoff = delivery
        .delivery
        .error
        .as_ref()
        .is_some_and(|e| e.code == DeliveryErrorCode::HeldForHandoff);
    if held_for_handoff || delivery.delivery.state == DeliveryState::Pending {
        return Ok(None);
    }
    if waits_for_answer(tx, member)? {
        return Ok(None);
    }
    let read = delivery.delivery.state == DeliveryState::Acked
        || (delivery.delivery.state == DeliveryState::HandedOff && line_holds_whole(&message.body));
    let reason = if !read {
        Reason::Unread
    } else if latest.is_none() {
        Reason::NoResult
    } else {
        return Ok(None);
    };
    Ok(Some(Corrective {
        display_id: message.display_id.to_string(),
        generation: member.generation,
        from_user: message.sender == Actor::User,
        reason,
    }))
}

/// Whether a `member.held` event of this generation already carries a
/// corrective hold.
fn already_held(tx: &ReadTx<'_>, member: &Member) -> Result<bool, StoreError> {
    let held = tx.events_of_kinds(&member.role, &[EventType::MemberHeld])?;
    Ok(held.iter().any(|e| {
        e.data
            .get("corrective")
            .and_then(|c| c.get("generation"))
            .and_then(Value::as_u64)
            == Some(u64::from(member.generation))
    }))
}

/// Whether the member asked a question in this generation that has no
/// answer yet: it waits, and a hold would talk over the wait.
fn waits_for_answer(tx: &ReadTx<'_>, member: &Member) -> Result<bool, StoreError> {
    let me = Actor::Member(member.role.clone());
    let open = tx.unanswered_questions()?;
    Ok(open
        .items
        .iter()
        .any(|q| q.sender == me && q.created_at >= member.gen_started_at))
}
