//! The watch's own rows: its lease (one watch per team) and the
//! announcements that keep it from saying a thing twice.

use rusqlite::Row;
use serde_json::{Value, json};
use teamwire_core::{EventType, Pid, Timestamp};

use crate::error::StoreError;
use crate::tx::{Listing, ReadTx, Tx, bad, col, listing, one, ts};

/// The `watch_lease` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchLease {
    pub pid: Pid,
    pub host: String,
    pub started_at: Timestamp,
    pub heartbeat_at: Timestamp,
}

/// One thing the watch said: its key (which carries a generation or episode
/// number) and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub key: String,
    pub at: Timestamp,
}

/// The answer to `take_watch_lease`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseOutcome {
    Taken,
    /// Another live watch holds the lease; this is its row.
    HeldBy(WatchLease),
}

fn lease_from_row(row: &Row<'_>) -> Result<WatchLease, String> {
    let pid: i64 = col(row, 0)?;
    Ok(WatchLease {
        pid: Pid::new(u32::try_from(pid).map_err(|e| bad("pid", e))?),
        host: col(row, 1)?,
        started_at: ts(row, 2)?,
        heartbeat_at: ts(row, 3)?,
    })
}

impl Tx<'_> {
    /// Takes the watch lease for `pid` and writes `watch.started`. When a
    /// row exists it is replaced only if it is this pid's own, or
    /// `holder_is_stale` says its holder is gone (the caller checks the pid
    /// with the platform; a quick check, made while the lock is held).
    pub fn take_watch_lease(
        &self,
        at: Timestamp,
        pid: Pid,
        host: &str,
        holder_is_stale: impl FnOnce(&WatchLease) -> bool,
    ) -> Result<LeaseOutcome, StoreError> {
        if let Some(held) = self.reader().watch_lease()?
            && held.pid != pid
            && !holder_is_stale(&held)
        {
            return Ok(LeaseOutcome::HeldBy(held));
        }
        self.connection().execute(
            "INSERT INTO watch_lease (id, pid, host, started_at, heartbeat_at)
             VALUES (1, ?1, ?2, ?3, ?3)
             ON CONFLICT (id) DO UPDATE SET pid = ?1, host = ?2, started_at = ?3, heartbeat_at = ?3",
            rusqlite::params![i64::from(pid.get()), host, at.to_string()],
        )?;
        self.record_event(
            at,
            EventType::WatchStarted,
            None,
            None,
            &json!({ "pid": pid.get(), "host": host }),
        )?;
        Ok(LeaseOutcome::Taken)
    }

    /// Refreshes the heartbeat. False when the lease is no longer `pid`'s:
    /// another watch took it, and this one must stop.
    pub fn heartbeat_watch_lease(&self, at: Timestamp, pid: Pid) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "UPDATE watch_lease SET heartbeat_at = ?2 WHERE id = 1 AND pid = ?1",
            rusqlite::params![i64::from(pid.get()), at.to_string()],
        )?;
        Ok(rows == 1)
    }

    /// Gives the lease up and writes `watch.stopped` with `data`. False when
    /// the lease is not `pid`'s (nothing changes, no event).
    pub fn release_watch_lease(
        &self,
        at: Timestamp,
        pid: Pid,
        data: &Value,
    ) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "DELETE FROM watch_lease WHERE id = 1 AND pid = ?1",
            [i64::from(pid.get())],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        self.record_event(at, EventType::WatchStopped, None, None, data)?;
        Ok(true)
    }

    /// Records that the watch said `key` and returns true, or returns false
    /// when it already had. The key carries a generation or episode number,
    /// so each thing is announced once. No event: the line itself is a
    /// `watch.line` event the watch writes.
    pub fn announce_once(&self, at: Timestamp, key: &str) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "INSERT OR IGNORE INTO announcements (key, at) VALUES (?1, ?2)",
            [key, &at.to_string()],
        )?;
        Ok(rows == 1)
    }
}

impl ReadTx<'_> {
    pub fn watch_lease(&self) -> Result<Option<WatchLease>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare("SELECT pid, host, started_at, heartbeat_at FROM watch_lease WHERE id = 1")?;
        one(listing("watch_lease", &mut stmt, [], lease_from_row)?)
    }

    /// Every announcement, oldest first (`teamwire export`).
    pub fn announcements(&self) -> Result<Listing<Announcement>, StoreError> {
        self.announcements_where("1 = 1", &[])
    }

    /// The announcements whose key starts with `prefix`, such as `SILENT:api:`,
    /// so a restarted watch finds the current episode number.
    pub fn announcements_with_prefix(
        &self,
        prefix: &str,
    ) -> Result<Listing<Announcement>, StoreError> {
        self.announcements_where("substr(key, 1, length(?1)) = ?1", &[prefix])
    }

    fn announcements_where(
        &self,
        condition: &str,
        args: &[&str],
    ) -> Result<Listing<Announcement>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT key, at FROM announcements WHERE {condition} ORDER BY rowid"
        ))?;
        listing(
            "announcements",
            &mut stmt,
            rusqlite::params_from_iter(args),
            |row| {
                Ok(Announcement {
                    key: col(row, 0)?,
                    at: ts(row, 1)?,
                })
            },
        )
    }
}

#[cfg(test)]
mod tests;
