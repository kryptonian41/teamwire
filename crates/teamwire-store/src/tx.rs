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
#[derive(Debug)]
pub struct Tx<'a> {
    conn: &'a Connection,
    writer_version: &'a str,
}

/// A read transaction, open inside `Store::read`.
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

    /// The connection this transaction runs on.
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

    /// The connection this transaction runs on.
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
