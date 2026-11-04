//! One poll, from the schema check to the line.

use teamwire_core::{OneLine, TeamState, Timestamp, one_line};
use teamwire_store::{Ctx, SCHEMA_VERSION, StoreError};

use crate::apply::apply;
use crate::memory::{WatchEnd, WatchMemory};
use crate::plan::plan;
use crate::snapshot;
use crate::text::{LINE_MAX, SEP, event};

/// This binary can never read the store again: waiting will not help.
fn is_final(e: &StoreError) -> bool {
    matches!(
        e,
        StoreError::SchemaNewer { .. } | StoreError::SchemaOlder { .. } | StoreError::NotTeamwire
    )
}

/// A newer binary may have migrated the file since the last poll, and this
/// binary would read a schema it does not know (ADR 0001).
fn check_schema(ctx: &Ctx<'_>) -> Result<(), StoreError> {
    let db = ctx.store.schema_version()?;
    match db.cmp(&SCHEMA_VERSION) {
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Greater => Err(StoreError::SchemaNewer {
            db,
            binary: SCHEMA_VERSION,
        }),
        std::cmp::Ordering::Less => Err(StoreError::SchemaOlder {
            db,
            binary: SCHEMA_VERSION,
        }),
    }
}

fn join(team: &str, at: Timestamp, texts: &[String]) -> OneLine {
    let events: Vec<String> = texts.iter().map(|t| event(team, at, t)).collect();
    line(&events)
}

/// Events that already carry their prefix, as one line.
fn line(events: &[String]) -> OneLine {
    one_line(&events.join(SEP), LINE_MAX)
}

/// The last line of a watch whose team was closed under it. The lead is
/// woken when the watch exits, so the exit carries one line that says why.
/// It is not stored: the team's state is closed or already gone.
fn closed(memory: &mut WatchMemory, now: Timestamp) -> OneLine {
    memory.end = Some(WatchEnd::TeamClosed);
    let team = memory.team.clone().unwrap_or_else(|| "unknown".to_owned());
    join(&team, now, &["TEAM-CLOSED".to_owned()])
}

fn step(
    ctx: &mut Ctx<'_>,
    memory: &mut WatchMemory,
    now: Timestamp,
) -> Result<Option<OneLine>, StoreError> {
    check_schema(ctx)?;
    let Some(snap) = snapshot::read(ctx)? else {
        return Ok(Some(closed(memory, now)));
    };
    memory.team = Some(snap.team.name.to_string());
    if snap.team.state == TeamState::Closed {
        return Ok(Some(closed(memory, now)));
    }
    let decided = plan(ctx, &snap, memory, now);
    let watching = (!memory.said_watching).then_some(snap.members.len());
    let applied = apply(
        ctx,
        decided,
        snap.team.name.as_ref(),
        memory.lease_pid,
        watching,
        now,
    )?;
    if applied.lease_lost {
        memory.end = Some(WatchEnd::LeaseLost);
        return Ok(None);
    }
    memory.said_watching = true;
    let events: Vec<String> = applied.emitted.iter().map(|e| e.text.clone()).collect();
    if applied.emitted.iter().any(|e| e.name == "TEAM-COMPLETE") {
        memory.end = Some(WatchEnd::TeamComplete);
    }
    Ok((!events.is_empty()).then(|| line(&events)))
}

/// One poll: at most one line, announcements stored before it is returned.
/// A failure to read or write the store is reported as `STORE-ERROR <code>`
/// once per code, and the next poll tries again unless the failure is final.
pub fn poll(ctx: &mut Ctx<'_>, memory: &mut WatchMemory) -> Option<OneLine> {
    if memory.end.is_some() {
        return None;
    }
    let now = ctx.clock.now();
    let outcome = step(ctx, memory, now);
    ctx.store.record_failure(now, "watch");
    memory.polls += 1;
    memory.last_poll = Some(now);
    memory.last_mono = Some(ctx.clock.monotonic());
    match outcome {
        Ok(line) => line,
        Err(e) => {
            tracing::warn!(error = %e, "the watch could not complete a poll");
            if is_final(&e) {
                memory.end = Some(WatchEnd::StoreError);
            }
            let code = e.code();
            if !memory.reported_errors.insert(code) {
                return None;
            }
            let team = memory.team.clone().unwrap_or_else(|| "unknown".to_owned());
            Some(join(&team, now, &[format!("STORE-ERROR {code}")]))
        }
    }
}
