//! One poll, from the schema check to the line.

use teamwire_core::{OneLine, TeamState, Timestamp, one_line};
use teamwire_store::{Ctx, SCHEMA_VERSION, StoreError};

use crate::apply::apply;
use crate::memory::{WatchEnd, WatchMemory};
use crate::plan::plan;
use crate::snapshot;
use crate::text::{LINE_MAX, event};

/// The short name of a store failure, as `STORE-ERROR <code>` prints it.
fn code(e: &StoreError) -> &'static str {
    match e {
        StoreError::Busy => "busy",
        StoreError::Full => "disk_full",
        StoreError::SchemaNewer { .. } => "schema_newer",
        StoreError::SchemaOlder { .. } => "schema_older",
        StoreError::NotTeamwire => "not_teamwire",
        StoreError::Corrupt(_) => "corrupt",
        StoreError::Constraint(_) => "constraint",
        StoreError::NoDatabase { .. } => "no_database",
        StoreError::Io(_) => "io",
    }
}

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

fn join(team: &str, texts: &[String]) -> OneLine {
    let events: Vec<String> = texts.iter().map(|t| event(team, t)).collect();
    one_line(&events.join(" | "), LINE_MAX)
}

fn step(
    ctx: &mut Ctx<'_>,
    memory: &mut WatchMemory,
    now: Timestamp,
) -> Result<Option<OneLine>, StoreError> {
    check_schema(ctx)?;
    let Some(snap) = snapshot::read(ctx)? else {
        memory.end = Some(WatchEnd::TeamClosed);
        return Ok(None);
    };
    memory.team = Some(snap.team.name.to_string());
    if snap.team.state == TeamState::Closed {
        memory.end = Some(WatchEnd::TeamClosed);
        return Ok(None);
    }
    let decided = plan(ctx, &snap, memory, now);
    let watching = (memory.polls == 0).then_some(snap.members.len());
    let applied = match apply(ctx, decided, memory.lease_pid, watching, now) {
        Ok(applied) => applied,
        Err(e) => {
            // Nothing was announced, so the stall (if any) was not said.
            memory.stalled = false;
            return Err(e);
        }
    };
    if applied.lease_lost {
        memory.end = Some(WatchEnd::LeaseLost);
        return Ok(None);
    }
    let texts: Vec<String> = applied.emitted.iter().map(|e| e.text.clone()).collect();
    if applied.emitted.iter().any(|e| e.name == "TEAM-COMPLETE") {
        memory.end = Some(WatchEnd::TeamComplete);
    }
    Ok((!texts.is_empty()).then(|| join(snap.team.name.as_ref(), &texts)))
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
    memory.polls += 1;
    memory.last_poll = Some(now);
    match outcome {
        Ok(line) => line,
        Err(e) => {
            tracing::warn!(error = %e, "the watch could not complete a poll");
            if is_final(&e) {
                memory.end = Some(WatchEnd::StoreError);
            }
            let code = code(&e);
            if !memory.reported_errors.insert(code) {
                return None;
            }
            let team = memory.team.clone().unwrap_or_else(|| "unknown".to_owned());
            Some(join(&team, &[format!("STORE-ERROR {code}")]))
        }
    }
}
