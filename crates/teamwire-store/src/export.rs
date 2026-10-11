//! The event log as the metric exporter reads it, and the exporter's claim
//! (ADR 0005 point 3).

use std::time::Duration;

use rusqlite::Row;
use serde_json::json;
use teamwire_core::{Event, EventType, Role, Timestamp, Ulid};

use crate::error::StoreError;
use crate::tx::{Listing, ReadTx, Tx, bad, col, listing, one, parsed, parsed_opt, ts, ulid_col};

/// The `metric_export` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportMark {
    /// Every event with `seq` up to this one is exported.
    pub last_event_seq: u64,
    pub claimed_to: Option<u64>,
    pub claimed_by: Option<String>,
    /// When the process named by `claimed_by` started, if it was known.
    pub claimed_by_started: Option<Timestamp>,
    pub claimed_at: Option<Timestamp>,
}

/// An exporter that holds or asks for the claim: its pid as text, and the
/// start time of that process when the platform could read it. The pid alone
/// may since belong to another process; the two together name the exporter.
/// A bare `&str` has no start time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportHolder {
    pub id: String,
    pub started: Option<Timestamp>,
}

impl From<&str> for ExportHolder {
    fn from(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            started: None,
        }
    }
}

/// A batch this exporter may send: events with `from_seq < seq <= to_seq`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportClaim {
    pub from_seq: u64,
    pub to_seq: u64,
    /// True when the batch was another exporter's, left after it died.
    pub taken_over: bool,
}

const EVENT_COLS: &str = "seq, ulid, time, type, role, subject, data, writer_version";

fn event_from_row(row: &Row<'_>) -> Result<Event, String> {
    let seq: i64 = col(row, 0)?;
    let data: String = col(row, 6)?;
    Ok(Event {
        seq: u64::try_from(seq).map_err(|e| bad("seq", e))?,
        ulid: ulid_col(row, 1)?,
        time: ts(row, 2)?,
        kind: parsed(row, 3, "type", EventType::parse)?,
        role: parsed_opt(row, 4, "role", Role::parse)?,
        subject: parsed_opt(row, 5, "subject", Ulid::parse)?,
        data: serde_json::from_str(&data).map_err(|e| bad("data", e))?,
        writer_version: col(row, 7)?,
    })
}

fn u64_col(row: &Row<'_>, i: usize) -> Result<Option<u64>, String> {
    let raw: Option<i64> = col(row, i)?;
    raw.map(|v| u64::try_from(v).map_err(|e| bad("seq", e)))
        .transpose()
}

fn mark_from_row(row: &Row<'_>) -> Result<ExportMark, String> {
    Ok(ExportMark {
        last_event_seq: u64_col(row, 0)?.unwrap_or(0),
        claimed_to: u64_col(row, 1)?,
        claimed_by: col(row, 2)?,
        claimed_by_started: crate::tx::ts_opt(row, 4)?,
        claimed_at: crate::tx::ts_opt(row, 3)?,
    })
}

fn as_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

impl Tx<'_> {
    /// Claims the next batch of events for `claimer` with one conditional
    /// update. `None` when there is nothing new, or when another exporter
    /// holds a claim. A claim older than `stale_after` whose holder (pid and
    /// start time) `holder_is_dead` reports gone is taken over, and the
    /// batch keeps its old end, so it is sent again once. No event: an
    /// event here would make the export chase its own tail.
    pub fn claim_metric_export(
        &self,
        at: Timestamp,
        claimer: impl Into<ExportHolder>,
        stale_after: Duration,
        holder_is_dead: impl FnOnce(&ExportHolder) -> bool,
    ) -> Result<Option<ExportClaim>, StoreError> {
        let claimer = claimer.into();
        self.connection().execute(
            "INSERT OR IGNORE INTO metric_export (id, last_event_seq) VALUES (1, 0)",
            [],
        )?;
        let reader = self.reader();
        let mark = reader.export_mark()?;
        let newest = reader.last_event_seq()?;
        let (to, taken_over) = match (&mark.claimed_to, &mark.claimed_by, mark.claimed_at) {
            (Some(to), Some(holder), Some(since)) => {
                let old = at.saturating_since(since) >= stale_after;
                let holder = ExportHolder {
                    id: holder.clone(),
                    started: mark.claimed_by_started,
                };
                if !(old && holder_is_dead(&holder)) {
                    return Ok(None);
                }
                (*to, true)
            }
            (Some(to), _, _) => (*to, true),
            _ => (newest, false),
        };
        if to <= mark.last_event_seq {
            return Ok(None);
        }
        let rows = self.connection().execute(
            "UPDATE metric_export SET claimed_to = ?1, claimed_by = ?2, claimed_at = ?3,
                    claimed_by_started_at = ?6
             WHERE id = 1 AND last_event_seq = ?4
               AND (claimed_to IS NULL OR claimed_to = ?5)",
            rusqlite::params![
                as_i64(to),
                claimer.id,
                at.to_string(),
                as_i64(mark.last_event_seq),
                as_i64(mark.claimed_to.unwrap_or(to)),
                claimer.started.map(|t| t.to_string()),
            ],
        )?;
        if rows == 0 {
            return Ok(None);
        }
        Ok(Some(ExportClaim {
            from_seq: mark.last_event_seq,
            to_seq: to,
            taken_over,
        }))
    }

    /// The batch was sent: moves the mark to `to_seq` and clears the claim.
    /// False when the claim is no longer this claimer's (it was taken over),
    /// and then the mark stays. The pid alone identifies the claimer here:
    /// only a running exporter commits, and no two running processes share
    /// a pid.
    pub fn commit_metric_export(&self, claimer: &str, to_seq: u64) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "UPDATE metric_export SET last_event_seq = claimed_to, claimed_to = NULL,
                    claimed_by = NULL, claimed_at = NULL, claimed_by_started_at = NULL
             WHERE id = 1 AND claimed_by = ?1 AND claimed_to = ?2",
            rusqlite::params![claimer, as_i64(to_seq)],
        )?;
        Ok(rows == 1)
    }

    /// The send failed: drops this claimer's claim so the next run retries at
    /// once, without waiting for it to go stale. The mark does not move.
    pub fn release_metric_claim(&self, claimer: &str) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "UPDATE metric_export SET claimed_to = NULL, claimed_by = NULL, claimed_at = NULL,
                    claimed_by_started_at = NULL
             WHERE id = 1 AND claimed_by = ?1",
            [claimer],
        )?;
        Ok(rows == 1)
    }

    /// A store failure, as the `store.error` event of `docs/metrics.md`.
    /// Call it in its own transaction after the failed one rolled back.
    pub fn record_store_error(
        &self,
        at: Timestamp,
        op: &str,
        error: &StoreError,
    ) -> Result<(), StoreError> {
        let code = match error {
            StoreError::Busy => "busy",
            StoreError::Full => "full",
            StoreError::SchemaNewer { .. } => "schema_newer",
            StoreError::SchemaOlder { .. } => "schema_older",
            StoreError::NotTeamwire => "not_teamwire",
            StoreError::Corrupt(_) => "corrupt",
            StoreError::Constraint(_) => "constraint",
            StoreError::NoDatabase { .. } | StoreError::Io(_) => "io",
        };
        self.record_event(
            at,
            EventType::StoreError,
            None,
            None,
            &json!({ "op": op, "code": code }),
        )?;
        Ok(())
    }
}

impl ReadTx<'_> {
    /// The exporter's mark; all zero before the first export.
    pub fn export_mark(&self) -> Result<ExportMark, StoreError> {
        let mut stmt = self.connection().prepare(
            "SELECT last_event_seq, claimed_to, claimed_by, claimed_at, claimed_by_started_at
             FROM metric_export WHERE id = 1",
        )?;
        Ok(
            one(listing("metric_export", &mut stmt, [], mark_from_row)?)?.unwrap_or(ExportMark {
                last_event_seq: 0,
                claimed_to: None,
                claimed_by: None,
                claimed_by_started: None,
                claimed_at: None,
            }),
        )
    }

    /// The highest event `seq`, or 0 with no events.
    pub fn last_event_seq(&self) -> Result<u64, StoreError> {
        let v: i64 =
            self.connection()
                .query_row("SELECT coalesce(max(seq), 0) FROM events", [], |r| r.get(0))?;
        u64::try_from(v).map_err(|_| StoreError::bad_value("events", "max", v))
    }

    /// Events with `after_seq < seq <= up_to_seq` (no upper bound when
    /// `None`), oldest first. A damaged row is reported in `bad`.
    pub fn events_after(
        &self,
        after_seq: u64,
        up_to_seq: Option<u64>,
        limit: Option<u32>,
    ) -> Result<Listing<Event>, StoreError> {
        let limit = limit.map_or_else(String::new, |n| format!(" LIMIT {n}"));
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {EVENT_COLS} FROM events WHERE seq > ?1 AND seq <= ?2 ORDER BY seq{limit}"
        ))?;
        listing(
            "events",
            &mut stmt,
            [
                as_i64(after_seq),
                as_i64(up_to_seq.unwrap_or(u64::MAX >> 1)),
            ],
            event_from_row,
        )
    }

    /// Events at or after `since`, oldest first (`metrics --since`).
    pub fn events_since(&self, since: Timestamp) -> Result<Listing<Event>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {EVENT_COLS} FROM events WHERE time >= ?1 ORDER BY seq"
        ))?;
        listing("events", &mut stmt, [since.to_string()], event_from_row)
    }

    /// The events written for one role, oldest first.
    pub fn events_of_role(&self, role: &Role) -> Result<Listing<Event>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {EVENT_COLS} FROM events WHERE role = ?1 ORDER BY seq"
        ))?;
        listing("events", &mut stmt, [role.as_str()], event_from_row)
    }

    /// One event by its ULID: how a span's start is found from its end.
    pub fn event(&self, ulid: &Ulid) -> Result<Option<Event>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare(&format!("SELECT {EVENT_COLS} FROM events WHERE ulid = ?1"))?;
        one(listing(
            "events",
            &mut stmt,
            [ulid.to_string()],
            event_from_row,
        )?)
    }

    /// Events of one type, oldest first.
    pub fn events_of_type(&self, kind: EventType) -> Result<Listing<Event>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {EVENT_COLS} FROM events WHERE type = ?1 ORDER BY seq"
        ))?;
        listing("events", &mut stmt, [kind.as_str()], event_from_row)
    }

    /// The events whose `subject` is `subject`, oldest first.
    pub fn events_about(&self, subject: &Ulid) -> Result<Listing<Event>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {EVENT_COLS} FROM events WHERE subject = ?1 ORDER BY seq"
        ))?;
        listing("events", &mut stmt, [subject.to_string()], event_from_row)
    }
}

#[cfg(test)]
mod tests;
