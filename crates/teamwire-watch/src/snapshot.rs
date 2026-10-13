//! Everything one poll reads from the store, in one read transaction so the
//! rows agree with each other. No external call happens inside it.

use std::collections::HashSet;
use std::path::PathBuf;

use teamwire_core::{
    Actor, DeliveryState, EventType, Member, MemberState, MessageType, Recipient, Timestamp, Ulid,
};
use teamwire_store::{CompletionRecord, Ctx, ReadTx, StoreError, TeamRow, database_path};

use crate::text::{STALLS_ENDED, blocked_prefix, silent_prefix};

pub(crate) struct Snapshot {
    pub team: TeamRow,
    pub team_dir: PathBuf,
    /// Members still to be waited for: launching or running.
    pub members: Vec<MemberInfo>,
    pub questions: Vec<Question>,
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

fn member_info(tx: &ReadTx<'_>, member: Member) -> Result<MemberInfo, StoreError> {
    let started = tx
        .events_of_role(&member.role)?
        .all_or_corrupt()?
        .into_iter()
        .rev()
        .find(|e| e.kind == EventType::MemberStarted)
        .map(|e| e.time);
    // A first generation counts from its start event, since `gen_started_at`
    // is the `creating` time. A later one counts from its reprompt, which is
    // after the start event.
    let run_start = started.map_or(member.gen_started_at, |s| s.max(member.gen_started_at));
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
            announced,
            stalls_ended: tx.counter(STALLS_ENDED)?.unwrap_or(0),
        }))
    })
}
