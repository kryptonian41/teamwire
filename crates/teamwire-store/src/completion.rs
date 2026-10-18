//! Completions: a member saying it is done, and how the result reaches the
//! lead (`pending`, `delivering`, then an outcome).

use std::time::Duration;

use rusqlite::Row;
use serde_json::{Value, json};
use teamwire_core::{
    Completion, CompletionStatus, DeliveryError, DeliveryErrorCode, EventType, FinalSource,
    ReportState, Role, Route, Timestamp, Ulid,
};

use crate::error::StoreError;
use crate::tx::{
    Listing, ReadTx, Tx, bad, col, hash32, listing, one, parsed, parsed_opt, ts, ts_opt, ulid_col,
};

/// A completion to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCompletion {
    pub role: Role,
    pub generation: u32,
    pub status: CompletionStatus,
    pub summary: String,
    pub final_message: Option<String>,
    pub final_source: FinalSource,
}

/// A completion and the columns core's `Completion` leaves out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRecord {
    pub completion: Completion,
    pub delivering_since: Option<Timestamp>,
    pub result_sha256: Option<[u8; 32]>,
}

/// How a result's delivery ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultOutcome {
    /// `Delivered`, `Undelivered` or `Unconfirmed`.
    pub state: ReportState,
    pub route: Option<Route>,
    pub error: Option<DeliveryError>,
    /// SHA-256 of the result file, once one was written.
    pub result_sha256: Option<[u8; 32]>,
}

const COMPLETION_COLS: &str = "ulid, role, generation, n, status, summary, final_message, \
     final_source, report_state, delivering_since, route, reason, reason_detail, result_sha256, \
     created_at";

fn completion_from_row(row: &Row<'_>) -> Result<CompletionRecord, String> {
    let generation: i64 = col(row, 2)?;
    let n: i64 = col(row, 3)?;
    let code: Option<String> = col(row, 11)?;
    let detail: Option<String> = col(row, 12)?;
    let reason = match code {
        Some(code) => Some(DeliveryError {
            code: DeliveryErrorCode::parse(&code).map_err(|e| bad("reason", e))?,
            detail: detail.unwrap_or_default(),
        }),
        None => None,
    };
    Ok(CompletionRecord {
        completion: Completion {
            ulid: ulid_col(row, 0)?,
            role: parsed(row, 1, "role", Role::parse)?,
            generation: u32::try_from(generation).map_err(|e| bad("generation", e))?,
            n: u32::try_from(n).map_err(|e| bad("n", e))?,
            status: parsed(row, 4, "status", CompletionStatus::parse)?,
            summary: col(row, 5)?,
            final_message: col(row, 6)?,
            final_source: parsed(row, 7, "final source", FinalSource::parse)?,
            report_state: parsed(row, 8, "report state", ReportState::parse)?,
            route: parsed_opt(row, 10, "route", Route::parse)?,
            reason,
            created_at: ts(row, 14)?,
        },
        delivering_since: ts_opt(row, 9)?,
        result_sha256: hash32(row, 13)?,
    })
}

fn timestamp_before(at: Timestamp, span: Duration) -> Timestamp {
    let millis = i64::try_from(span.as_millis()).unwrap_or(i64::MAX);
    Timestamp::from_unix_millis(at.unix_millis().saturating_sub(millis)).unwrap_or(at)
}

fn seconds_between(from: Timestamp, to: Timestamp) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "milliseconds between two events fit an f64 exactly for any real span"
    )]
    let millis = to.saturating_since(from).as_millis() as f64;
    millis / 1000.0
}

impl Tx<'_> {
    /// Stores a completion as `pending`, numbering it with the role's own
    /// counter, and writes `member.completed`.
    pub fn insert_completion(
        &self,
        at: Timestamp,
        new: &NewCompletion,
    ) -> Result<Completion, StoreError> {
        let n = self.next_counter(&format!("completion:{}", new.role))?;
        let ulid = self.new_ulid(at);
        self.connection().execute(
            "INSERT INTO completions (ulid, role, generation, n, status, summary, final_message,
                                      final_source, report_state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9)",
            rusqlite::params![
                ulid.to_string(),
                new.role.as_str(),
                new.generation,
                i64::try_from(n).unwrap_or(i64::MAX),
                new.status.as_str(),
                new.summary,
                new.final_message,
                new.final_source.as_str(),
                at.to_string(),
            ],
        )?;
        self.record_event(
            at,
            EventType::MemberCompleted,
            Some(&new.role),
            Some(&ulid),
            &json!({ "status": new.status.as_str(), "generation": new.generation, "n": n }),
        )?;
        self.reader()
            .completion(&ulid)?
            .map(|r| r.completion)
            .ok_or_else(|| StoreError::Corrupt(format!("completion {ulid} vanished")))
    }

    /// Records the member's final message and where it came from (`hook` or
    /// `transcript`) on a completion stored earlier, at stop time (REP-7).
    /// False when there is no such completion. No event: `member.completed`
    /// was written when the completion was stored.
    pub fn set_completion_final(
        &self,
        completion: &Ulid,
        final_message: Option<&str>,
        source: FinalSource,
    ) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "UPDATE completions SET final_message = ?2, final_source = ?3 WHERE ulid = ?1",
            rusqlite::params![completion.to_string(), final_message, source.as_str()],
        )?;
        Ok(rows == 1)
    }

    /// Route B (ADR 0007): the watch reports a `pending` completion, so it goes
    /// straight to `delivered` with route `watch`. Writes one
    /// `result.delivered` event and no `result.delivering`. False when the
    /// completion is not `pending`.
    pub fn deliver_completion_via_watch(
        &self,
        at: Timestamp,
        completion: &Ulid,
    ) -> Result<bool, StoreError> {
        let Some(current) = self.reader().completion(completion)? else {
            return Ok(false);
        };
        let rows = self.connection().execute(
            "UPDATE completions SET report_state = 'delivered', route = 'watch'
             WHERE ulid = ?1 AND report_state = 'pending'",
            [completion.to_string()],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        let c = &current.completion;
        self.record_event(
            at,
            EventType::ResultDelivered,
            Some(&c.role),
            Some(completion),
            &json!({
                "route": Route::Watch.as_str(),
                "code": null,
                "seconds": seconds_between(c.created_at, at),
            }),
        )?;
        Ok(true)
    }

    /// Takes a result for delivery: one conditional update into `delivering`
    /// that records `delivering_since`, and writes `result.delivering`.
    /// Takes a `pending` or `undelivered` result, and a `delivering` one
    /// older than `stale_after` (its hook died). Returns the state it was
    /// taken from, or `None` when another caller holds it or it is finished.
    pub fn claim_completion_for_delivery(
        &self,
        at: Timestamp,
        completion: &Ulid,
        stale_after: Duration,
    ) -> Result<Option<ReportState>, StoreError> {
        self.claim_completion_for_delivery_with(at, completion, stale_after, &Value::Null)
    }

    /// `claim_completion_for_delivery`, and the keys of the object `extra`
    /// are added to the `result.delivering` event's data.
    pub fn claim_completion_for_delivery_with(
        &self,
        at: Timestamp,
        completion: &Ulid,
        stale_after: Duration,
        extra: &Value,
    ) -> Result<Option<ReportState>, StoreError> {
        let Some(current) = self.reader().completion(completion)? else {
            return Ok(None);
        };
        let rows = self.connection().execute(
            "UPDATE completions SET report_state = 'delivering', delivering_since = ?2
             WHERE ulid = ?1 AND (report_state IN ('pending', 'undelivered')
                                  OR (report_state = 'delivering' AND delivering_since < ?3))",
            rusqlite::params![
                completion.to_string(),
                at.to_string(),
                timestamp_before(at, stale_after).to_string(),
            ],
        )?;
        if rows == 0 {
            return Ok(None);
        }
        let c = &current.completion;
        let mut data = extra.as_object().cloned().unwrap_or_default();
        data.insert("route".to_owned(), json!(c.route.map(Route::as_str)));
        data.insert(
            "seconds".to_owned(),
            json!(seconds_between(c.created_at, at)),
        );
        self.record_event(
            at,
            EventType::ResultDelivering,
            Some(&c.role),
            Some(completion),
            &Value::Object(data),
        )?;
        Ok(Some(c.report_state))
    }

    /// Ends a delivery the caller claimed: `delivering` to `delivered`,
    /// `undelivered` or `unconfirmed`, and writes the matching `result.*`
    /// event. False, and nothing written, when the result is no longer
    /// `delivering` (for example the watch already marked it interrupted).
    pub fn finish_completion_delivery(
        &self,
        at: Timestamp,
        completion: &Ulid,
        outcome: &ResultOutcome,
    ) -> Result<bool, StoreError> {
        self.finish_completion_delivery_with(at, completion, outcome, &Value::Null)
    }

    /// `finish_completion_delivery`, with `extra` (a JSON object) merged into
    /// the `result.*` event's data, such as a hook run's `hook` and `hook_ms`
    /// (`docs/metrics.md` section 5). The fields this method writes win.
    pub fn finish_completion_delivery_with(
        &self,
        at: Timestamp,
        completion: &Ulid,
        outcome: &ResultOutcome,
        extra: &Value,
    ) -> Result<bool, StoreError> {
        let event = match outcome.state {
            ReportState::Delivered => EventType::ResultDelivered,
            ReportState::Undelivered => EventType::ResultUndelivered,
            ReportState::Unconfirmed => EventType::ResultUnconfirmed,
            ReportState::Pending | ReportState::Delivering => {
                return Err(StoreError::Constraint(format!(
                    "a delivery cannot end as {}",
                    outcome.state
                )));
            }
        };
        let Some(current) = self.reader().completion(completion)? else {
            return Ok(false);
        };
        let rows = self.connection().execute(
            "UPDATE completions SET report_state = ?2, route = ?3, reason = ?4, reason_detail = ?5,
                    result_sha256 = coalesce(?6, result_sha256)
             WHERE ulid = ?1 AND report_state = 'delivering'",
            rusqlite::params![
                completion.to_string(),
                outcome.state.as_str(),
                outcome.route.map(Route::as_str),
                outcome.error.as_ref().map(|e| e.code.as_str()),
                outcome.error.as_ref().map(|e| e.detail.as_str()),
                outcome.result_sha256.as_ref().map(<[u8; 32]>::as_slice),
            ],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        let c = &current.completion;
        let mut data = extra.as_object().cloned().unwrap_or_default();
        data.insert("route".to_owned(), json!(outcome.route.map(Route::as_str)));
        data.insert(
            "code".to_owned(),
            json!(outcome.error.as_ref().map(|e| e.code.as_str())),
        );
        data.insert(
            "seconds".to_owned(),
            json!(seconds_between(c.created_at, at)),
        );
        self.record_event(
            at,
            event,
            Some(&c.role),
            Some(completion),
            &Value::Object(data),
        )?;
        Ok(true)
    }

    /// The watch's conditional update (ADR 0007 point 6a): a result left in
    /// `delivering` for longer than `stale_after` becomes `undelivered` with
    /// `delivery_interrupted`. False when it is not `delivering`, or not old
    /// enough, so two watches never both announce it.
    pub fn interrupt_stale_delivery(
        &self,
        at: Timestamp,
        completion: &Ulid,
        stale_after: Duration,
    ) -> Result<bool, StoreError> {
        let Some(current) = self.reader().completion(completion)? else {
            return Ok(false);
        };
        let rows = self.connection().execute(
            "UPDATE completions SET report_state = 'undelivered', reason = 'delivery_interrupted',
                    reason_detail = 'the hook that was delivering this result did not finish'
             WHERE ulid = ?1 AND report_state = 'delivering' AND delivering_since < ?2",
            rusqlite::params![
                completion.to_string(),
                timestamp_before(at, stale_after).to_string()
            ],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        let c = &current.completion;
        self.record_event(
            at,
            EventType::ResultUndelivered,
            Some(&c.role),
            Some(completion),
            &json!({
                "route": null,
                "code": DeliveryErrorCode::DeliveryInterrupted.as_str(),
                "seconds": seconds_between(c.created_at, at),
            }),
        )?;
        Ok(true)
    }

    /// `teamwire received` (ADR 0024): the lead read an `undelivered` or
    /// `unconfirmed` result from its file, so it becomes `delivered` with
    /// route `inbox`, and one `result.delivered` event is written. The
    /// reason stays as the record of why the typing failed. False when the
    /// result is in any other state.
    pub fn mark_result_received(
        &self,
        at: Timestamp,
        completion: &Ulid,
    ) -> Result<bool, StoreError> {
        let Some(current) = self.reader().completion(completion)? else {
            return Ok(false);
        };
        let rows = self.connection().execute(
            "UPDATE completions SET report_state = 'delivered', route = 'inbox'
             WHERE ulid = ?1 AND report_state IN ('undelivered', 'unconfirmed')",
            [completion.to_string()],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        let c = &current.completion;
        self.record_event(
            at,
            EventType::ResultDelivered,
            Some(&c.role),
            Some(completion),
            &json!({
                "route": Route::Inbox.as_str(),
                "code": null,
                "seconds": seconds_between(c.created_at, at),
            }),
        )?;
        Ok(true)
    }
}

impl ReadTx<'_> {
    pub fn completion(&self, ulid: &Ulid) -> Result<Option<CompletionRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {COMPLETION_COLS} FROM completions WHERE ulid = ?1"
        ))?;
        one(listing(
            "completions",
            &mut stmt,
            [ulid.to_string()],
            completion_from_row,
        )?)
    }

    /// Every completion of every role, by role then number
    /// (`teamwire export`). A damaged member row cannot hide a completion,
    /// because no member row is read.
    pub fn completions(&self) -> Result<Listing<CompletionRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {COMPLETION_COLS} FROM completions ORDER BY role, n"
        ))?;
        listing("completions", &mut stmt, [], completion_from_row)
    }

    /// A member's completions, oldest first.
    pub fn completions_of(&self, role: &Role) -> Result<Listing<CompletionRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {COMPLETION_COLS} FROM completions WHERE role = ?1 ORDER BY n"
        ))?;
        listing(
            "completions",
            &mut stmt,
            [role.as_str()],
            completion_from_row,
        )
    }

    /// A member's newest completion.
    pub fn latest_completion(&self, role: &Role) -> Result<Option<CompletionRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {COMPLETION_COLS} FROM completions WHERE role = ?1 ORDER BY n DESC LIMIT 1"
        ))?;
        one(listing(
            "completions",
            &mut stmt,
            [role.as_str()],
            completion_from_row,
        )?)
    }

    /// Every completion whose result is in one of `states`, oldest first.
    pub fn completions_in_states(
        &self,
        states: &[ReportState],
    ) -> Result<Listing<CompletionRecord>, StoreError> {
        let list = states
            .iter()
            .map(|s| format!("'{}'", s.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {COMPLETION_COLS} FROM completions WHERE report_state IN ({list}) ORDER BY rowid"
        ))?;
        listing("completions", &mut stmt, [], completion_from_row)
    }
}

#[cfg(test)]
mod tests;
