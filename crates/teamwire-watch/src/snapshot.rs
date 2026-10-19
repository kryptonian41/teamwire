//! Everything one poll reads from the store, in one read transaction so the
//! rows agree with each other. No external call happens inside it.

use std::collections::HashSet;
use std::path::PathBuf;

use teamwire_core::{
    Actor, DeliveryState, EventType, Member, MemberState, MessageType, Recipient, Role, Timestamp,
    Ulid,
};
use teamwire_deliver::unread_since;
use teamwire_store::{CompletionRecord, Ctx, ReadTx, StoreError, TeamRow, database_path};

use crate::text::{STALLS_ENDED, blocked_prefix, silent_prefix};

pub(crate) struct Snapshot {
    pub team: TeamRow,
    pub team_dir: PathBuf,
    /// Members still to be waited for: launching or running.
    pub members: Vec<MemberInfo>,
    pub questions: Vec<Question>,
    /// Deliveries to a member that the host took and nobody acknowledged,
    /// leaving out a message whose whole text the typed line carried.
    pub unread: Vec<Unread>,
    /// Questions to the lead that no answer points at.
    pub unanswered: Vec<Unanswered>,
    /// Every key the watch has announced, in this or an earlier process.
    pub announced: HashSet<String>,
    /// How many `TEAM-STALLED` episodes have ended. Episodes are numbered in
    /// order, so the latest one is open when its number is above this.
    pub stalls_ended: u64,
}

pub(crate) struct MemberInfo {
    pub member: Member,
    /// When the member's current run began: its start for the first
    /// generation, the reprompt for a later one.
    pub run_start: Timestamp,
    /// Its newest completion in the current generation.
    pub completion: Option<CompletionRecord>,
    /// The highest `SILENT` and `BLOCKED` episode already announced for the
    /// current generation.
    pub silent_seen: u32,
    pub blocked_seen: u32,
}

/// A question to the lead that did not get there cleanly.
pub(crate) struct Question {
    pub message: Ulid,
    pub display_id: String,
    pub sender: Actor,
    pub state: DeliveryState,
}

pub(crate) struct Unread {
    pub message: Ulid,
    pub display_id: String,
    pub to: Role,
    /// When the latest attempt at this delivery began.
    pub since: Timestamp,
}

pub(crate) struct Unanswered {
    pub message: Ulid,
    pub display_id: String,
    pub sender: Actor,
    pub since: Timestamp,
}

/// The highest episode number among announced keys `<prefix><n>`.
fn highest_episode(tx: &ReadTx<'_>, prefix: &str) -> Result<u32, StoreError> {
    Ok(tx
        .announcements_with_prefix(prefix)?
        .all_or_corrupt()?
        .iter()
        .filter_map(|a| a.key.strip_prefix(prefix)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0))
}

/// When the member's current run began, given the time of its latest
/// `member.started` event. A first generation counts from that event, since
/// `gen_started_at` is the `creating` time. A later one counts from its
/// reprompt, which is after the start event.
pub(crate) fn run_start(member: &Member, started: Option<Timestamp>) -> Timestamp {
    started.map_or(member.gen_started_at, |s| s.max(member.gen_started_at))
}

fn member_info(tx: &ReadTx<'_>, member: Member) -> Result<MemberInfo, StoreError> {
    let started = tx
        .events_of_role(&member.role)?
        .all_or_corrupt()?
        .into_iter()
        .rev()
        .find(|e| e.kind == EventType::MemberStarted)
        .map(|e| e.time);
    let run_start = run_start(&member, started);
    let completion = tx
        .latest_completion(&member.role)?
        .filter(|c| c.completion.generation == member.generation);
    Ok(MemberInfo {
        silent_seen: highest_episode(tx, &silent_prefix(&member.role, member.generation))?,
        blocked_seen: highest_episode(tx, &blocked_prefix(&member.role, member.generation))?,
        member,
        run_start,
        completion,
    })
}

fn questions(tx: &ReadTx<'_>) -> Result<Vec<Question>, StoreError> {
    let mut found = Vec::new();
    let stuck = tx
        .deliveries_in_states(&[DeliveryState::Undelivered, DeliveryState::Unconfirmed])?
        .all_or_corrupt()?;
    for record in stuck {
        let d = record.delivery;
        if d.recipient != Recipient::Lead {
            continue;
        }
        let Some(message) = tx.message(&d.message)? else {
            continue;
        };
        // A question from a member of this team or from any agent of
        // another team waits on this lead; the lead's own never does.
        if message.kind == MessageType::Question && message.sender != Actor::Lead {
            found.push(Question {
                message: message.ulid,
                display_id: message.display_id.to_string(),
                sender: message.sender.clone(),
                state: d.state,
            });
        }
    }
    Ok(found)
}

fn unread(tx: &ReadTx<'_>) -> Result<Vec<Unread>, StoreError> {
    let mut found = Vec::new();
    for record in tx
        .deliveries_in_states(&[DeliveryState::HandedOff])?
        .all_or_corrupt()?
    {
        let d = record.delivery;
        // The lead reads its own inbox; only a member's silence is news.
        let Recipient::Member(to) = d.recipient.clone() else {
            continue;
        };
        let Some(message) = tx.message(&d.message)? else {
            continue;
        };
        let Some(since) = unread_since(tx, &message, &d.recipient)? else {
            continue;
        };
        found.push(Unread {
            since,
            message: message.ulid,
            display_id: message.display_id.to_string(),
            to,
        });
    }
    Ok(found)
}

fn unanswered(tx: &ReadTx<'_>) -> Result<Vec<Unanswered>, StoreError> {
    let mut found = Vec::new();
    for message in tx.unanswered_questions()?.all_or_corrupt()? {
        // A question the lead never received is `QUESTION-UNDELIVERED`'s;
        // questions between members are for `status` only.
        let reached_lead = tx
            .delivery(&message.ulid, &Recipient::Lead)?
            .is_some_and(|r| {
                matches!(
                    r.delivery.state,
                    DeliveryState::HandedOff | DeliveryState::Acked
                )
            });
        if reached_lead {
            found.push(Unanswered {
                message: message.ulid,
                display_id: message.display_id.to_string(),
                sender: message.sender,
                since: message.created_at,
            });
        }
    }
    Ok(found)
}

/// `Ok(None)` when the team has no row: there is nothing to watch.
pub(crate) fn read(ctx: &Ctx<'_>) -> Result<Option<Snapshot>, StoreError> {
    ctx.store.read(|tx| {
        let Some(team) = tx.team()? else {
            return Ok(None);
        };
        let team_dir = database_path(ctx.home, &team.name)
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut members = Vec::new();
        for member in tx.members()?.all_or_corrupt()? {
            if matches!(member.state, MemberState::Creating | MemberState::Started) {
                members.push(member_info(tx, member)?);
            }
        }
        let announced = tx
            .announcements()?
            .all_or_corrupt()?
            .into_iter()
            .map(|a| a.key)
            .collect();
        Ok(Some(Snapshot {
            team,
            team_dir,
            members,
            questions: questions(tx)?,
            unread: unread(tx)?,
            unanswered: unanswered(tx)?,
            announced,
            stalls_ended: tx.counter(STALLS_ENDED)?.unwrap_or(0),
        }))
    })
}
