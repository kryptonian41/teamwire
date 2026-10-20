//! The two transaction handles and what every typed method shares: event
//! rows, counters, new ULIDs, and reading rows that may be damaged.

use std::fmt::Display;
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Params, Row, Statement};
use serde_json::Value;
use teamwire_core::{EventType, Role, Timestamp, Ulid};

use crate::error::StoreError;

/// A write transaction, open inside `Store::write`. Every method that
/// changes a row takes the event time `at` from the caller, who got it from
/// `Clock::now`: the store never reads the clock.
///
/// The methods, by the file that holds them. Each writes its event row in the
/// same transaction, except the bookkeeping writes named below.
/// - team: `create_team`, `create_team_with`, `create_afk_team`, `begin_close`,
///   `finish_close`, `clear_stale_close_lock`, `adopt_lead`, `adopt_lead_in`.
/// - member: `insert_member`, `insert_member_with`, `relaunch_failed_member`,
///   `relaunch_failed_member_with`, `set_member_state`, `remove_member`,
///   `bump_generation`, `restore_generation`, `hold_member`,
///   `hold_member_unread`.
/// - message and delivery: `insert_message`, `insert_received`,
///   `set_delivery_state` (forward moves only), `ack_delivery`, `begin_resend`,
///   `mark_reminded`, `record_delivery_reentered`.
/// - completion: `insert_completion`, `claim_completion_for_delivery`,
///   `claim_completion_for_delivery_with`, `finish_completion_delivery`,
///   `finish_completion_delivery_with`, `deliver_completion_via_watch`,
///   `interrupt_stale_delivery`, `mark_result_received`.
/// - verify and watch: `insert_verify_record`, `insert_waiver`,
///   `take_watch_lease`, `release_watch_lease`.
/// - `record_event` writes any other event, `record_store_error` a
///   `store.error`; `next_counter` and `new_ulid` serve services that number
///   their own rows.
///
/// Bookkeeping writes that write no event, because the closed event list has
/// no type for them and none of them is a lifecycle or message change:
/// `set_sleep_hold`, `set_member_place`, `set_member_session`,
/// `set_member_token_hash`, `clear_member_branch`, `set_gate_told_generation`,
/// `set_completion_final`, `announce_once`, `heartbeat_watch_lease`, and the
/// metric-export `claim_metric_export`, `commit_metric_export`,
/// `release_metric_claim` (an event there would feed the export its own tail).
///
/// Data a hook or a launch adds to events, with no new event type: a hook
/// puts `hook_ms` in the data of the events it writes (the source of
/// `teamwire.hook.duration`); launch puts a launch id and the requested count
/// in the data of `member.creating`, `member.started` and `member.failed`.
#[derive(Debug)]
pub struct Tx<'a> {
    conn: &'a Connection,
    writer_version: &'a str,
}

/// A read transaction, open inside `Store::read`.
///
/// The reads, by the file that holds them:
/// - team and member: `team`; `member`, `members`, `member_token_hash`;
///   `lead_cwd`, `run_options` (read from event data).
/// - message: `message`, `message_by_display_id`, `message_by_client_key`,
///   `messages` (filter by sender, recipient, either, type, time, newest N),
///   `sent_to_since`, `inbox` (bodies with a hash flag), `unanswered_questions`,
///   `counter`, `counters`, `counter_floors`.
/// - delivery and completion: `delivery`, `deliveries_of`, `deliveries_for`,
///   `deliveries_in_states`; `completion`, `completions`, `completions_of`,
///   `latest_completion`, `completions_in_states`.
/// - verify and watch: `message_bodies` (raw rows, no hash refusal),
///   `verify_records`, `all_verify_records`, `waivers`, `all_waivers`;
///   `watch_lease`, `announcements`, `announcements_with_prefix`.
/// - events: `export_mark`, `last_event_seq`, `events_after`, `events_since`,
///   `events_of_role`, `event`, `events_of_type`, `events_about`;
///   `latest_event` and `events_of_kinds`, which read only the rows they
///   return.
#[derive(Debug)]
pub struct ReadTx<'a> {
    conn: &'a Connection,
}

impl<'a> Tx<'a> {
    pub(crate) fn new(conn: &'a Connection, writer_version: &'a str) -> Self {
        Self {
            conn,
            writer_version,
        }
    }

    /// The connection this transaction runs on. For tests only, to damage a
    /// row on purpose: production code uses the typed methods.
    #[doc(hidden)]
    pub fn connection(&self) -> &Connection {
        self.conn
    }

    /// The reads, inside this same transaction: a service checks a row and
    /// changes it under one lock.
    pub fn reader(&self) -> ReadTx<'a> {
        ReadTx::new(self.conn)
    }

    /// Writes one event row and returns its ULID. The typed methods call this
    /// for their own change; services call it for events with no row change
    /// (`message.refused`, `watch.line`, `close.step`, `store.error`, ...).
    pub fn record_event(
        &self,
        at: Timestamp,
        kind: EventType,
        role: Option<&Role>,
        subject: Option<&Ulid>,
        data: &Value,
    ) -> Result<Ulid, StoreError> {
        let ulid = fresh_ulid(at);
        self.conn.execute(
            "INSERT INTO events (ulid, time, type, role, subject, data, writer_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                ulid.to_string(),
                at.to_string(),
                kind.as_str(),
                role.map(Role::as_str),
                subject.map(Ulid::to_string),
                data.to_string(),
                self.writer_version,
            ],
        )?;
        Ok(ulid)
    }

    /// Adds one to the named counter (creating it at 1) and returns the new
    /// value. Display ids and per-sender `seq` come only from here. A counter
    /// that was damaged to a smaller value makes the next insert collide with
    /// an existing row and fail; it never hands out an id twice.
    pub fn next_counter(&self, name: &str) -> Result<u64, StoreError> {
        let value: i64 = self.conn.query_row(
            "INSERT INTO counters (name, value) VALUES (?1, 1)
             ON CONFLICT (name) DO UPDATE SET value = value + 1
             RETURNING value",
            [name],
            |row| row.get(0),
        )?;
        u64::try_from(value).map_err(|_| StoreError::bad_value("counters", name, value))
    }

    /// A ULID for `at`, for rows this transaction inserts.
    pub fn new_ulid(&self, at: Timestamp) -> Ulid {
        fresh_ulid(at)
    }
}

impl<'a> ReadTx<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// The connection this transaction runs on. For tests only, to damage a
    /// row on purpose: production code uses the typed methods.
    #[doc(hidden)]
    pub fn connection(&self) -> &Connection {
        self.conn
    }
}

/// A ULID made from `at` and 80 random bits. The time comes from the caller;
/// only the random part comes from the `ulid` crate.
pub(crate) fn fresh_ulid(at: Timestamp) -> Ulid {
    let millis = u64::try_from(at.unix_millis()).unwrap_or(0);
    let random = ulid::Ulid::from_datetime(UNIX_EPOCH + Duration::from_millis(millis)).random();
    Ulid::from_parts(at, random)
}

/// A row that could not be read back: named so the reader can report it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadRow {
    pub table: &'static str,
    pub key: String,
    pub reason: String,
}

/// The rows a query could read, and the rows it could not. A damaged row is
/// reported and skipped; it is never read as a success (STO-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub bad: Vec<BadRow>,
}

impl<T> Default for Listing<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            bad: Vec::new(),
        }
    }
}

impl<T> Listing<T> {
    /// The items, or the first damaged row as an error, for callers that
    /// cannot go on without every row.
    pub fn all_or_corrupt(self) -> Result<Vec<T>, StoreError> {
        match self.bad.into_iter().next() {
            Some(bad) => Err(StoreError::bad_value(bad.table, &bad.key, bad.reason)),
            None => Ok(self.items),
        }
    }
}

/// Runs `stmt` and parses each row. The first column of every query must be
/// the row's key; it names the row when parsing fails.
pub(crate) fn listing<T>(
    table: &'static str,
    stmt: &mut Statement<'_>,
    params: impl Params,
    parse: impl Fn(&Row<'_>) -> Result<T, String>,
) -> Result<Listing<T>, StoreError> {
    let mut out = Listing::default();
    let mut rows = stmt.query(params)?;
    while let Some(row) = rows.next()? {
        match parse(row) {
            Ok(item) => out.items.push(item),
            Err(reason) => {
                let key = match row.get_ref(0) {
                    Ok(ValueRef::Text(t)) => String::from_utf8_lossy(t).into_owned(),
                    Ok(ValueRef::Integer(i)) => i.to_string(),
                    _ => "?".to_owned(),
                };
                tracing::warn!(table, %key, %reason, "skipping a row that does not parse");
                out.bad.push(BadRow { table, key, reason });
            }
        }
    }
    Ok(out)
}

/// One row or none. A row that does not parse is an error, not "none".
pub(crate) fn one<T>(listing: Listing<T>) -> Result<Option<T>, StoreError> {
    Ok(listing.all_or_corrupt()?.into_iter().next())
}

/// Reads column `i`, turning the error into the text a `BadRow` carries.
pub(crate) fn col<T: rusqlite::types::FromSql>(row: &Row<'_>, i: usize) -> Result<T, String> {
    row.get::<_, T>(i).map_err(|e| format!("column {i}: {e}"))
}

/// Maps a parse error of a stored value to the text a `BadRow` carries.
pub(crate) fn bad<E: Display>(what: &str, e: E) -> String {
    format!("{what}: {e}")
}

/// Reads a stored time.
pub(crate) fn ts(row: &Row<'_>, i: usize) -> Result<Timestamp, String> {
    let text: String = col(row, i)?;
    Timestamp::parse(&text).map_err(|e| bad("time", e))
}

pub(crate) fn ts_opt(row: &Row<'_>, i: usize) -> Result<Option<Timestamp>, String> {
    let text: Option<String> = col(row, i)?;
    text.map(|t| Timestamp::parse(&t).map_err(|e| bad("time", e)))
        .transpose()
}

pub(crate) fn hash32(row: &Row<'_>, i: usize) -> Result<Option<[u8; 32]>, String> {
    let raw: Option<Vec<u8>> = col(row, i)?;
    raw.map(|v| <[u8; 32]>::try_from(v).map_err(|v| format!("hash of {} bytes", v.len())))
        .transpose()
}

pub(crate) fn ulid_col(row: &Row<'_>, i: usize) -> Result<Ulid, String> {
    let text: String = col(row, i)?;
    Ulid::parse(&text).map_err(|e| bad("ulid", e))
}

/// Fails with a constraint error when an UPDATE that had to hit one row hit none.
pub(crate) fn exactly_one(rows: usize, what: impl Display) -> Result<(), StoreError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(StoreError::Constraint(format!("no such {what}")))
    }
}

/// Parses a stored string with a core parser, naming the field on failure.
pub(crate) fn parsed<T, E: Display>(
    row: &Row<'_>,
    i: usize,
    what: &str,
    parse: impl Fn(&str) -> Result<T, E>,
) -> Result<T, String> {
    let text: String = col(row, i)?;
    parse(&text).map_err(|e| bad(what, e))
}

pub(crate) fn parsed_opt<T, E: Display>(
    row: &Row<'_>,
    i: usize,
    what: &str,
    parse: impl Fn(&str) -> Result<T, E>,
) -> Result<Option<T>, String> {
    let text: Option<String> = col(row, i)?;
    text.map(|t| parse(&t).map_err(|e| bad(what, e)))
        .transpose()
}

/// The 32-byte SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).into()
}

/// The SHA-256 of a token's text: what launch stores for a member and what a
/// caller's token is checked against (ADR 0008 point 1).
pub fn token_hash(token: &str) -> [u8; 32] {
    sha256(token.as_bytes())
}

/// True when two hashes are equal. Every byte is compared, so the time taken
/// does not tell a caller where a guessed hash first differs (ADR 0008).
pub fn same_hash(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::{same_hash, token_hash};

    #[test]
    fn same_hash_is_true_only_for_equal_bytes_of_equal_length() {
        let a = token_hash("secret");
        let mut b = a;
        b[31] ^= 1;
        assert!(same_hash(&a, &token_hash("secret")));
        assert!(!same_hash(&a, &b));
        assert!(!same_hash(&a, &token_hash("other")));
        assert!(!same_hash(&a, &a[..31]));
        assert!(!same_hash(&a[..31], &a));
        assert!(same_hash(&[], &[]));
    }
}
