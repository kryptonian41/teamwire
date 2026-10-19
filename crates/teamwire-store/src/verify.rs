//! Verify records and waivers.

use rusqlite::Row;
use serde_json::json;
use teamwire_core::{DisplayId, EventType, Role, Timestamp};

use crate::error::StoreError;
use crate::tx::{Listing, ReadTx, Tx, bad, col, hash32, listing, parsed, ts};

/// A verify run to record. The verdict is T09's own word for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVerifyRecord {
    pub role: Role,
    pub head_sha: String,
    pub verdict: String,
    pub exit_code: Option<i32>,
    pub seconds: Option<u64>,
    /// Flag ids the run raised.
    pub flags: Vec<String>,
    pub log_sha256: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyRow {
    pub id: i64,
    pub record: NewVerifyRecord,
    pub recorded_at: Timestamp,
}

/// A lead's decision to accept one flag on one head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiver {
    pub role: Role,
    pub flag_id: String,
    pub head_sha: String,
    pub reason: String,
    pub created_at: Timestamp,
}

const VERIFY_COLS: &str =
    "id, role, head_sha, verdict, exit_code, seconds, flags, log_sha256, recorded_at";

fn verify_from_row(row: &Row<'_>) -> Result<VerifyRow, String> {
    let flags: String = col(row, 6)?;
    let seconds: Option<i64> = col(row, 5)?;
    Ok(VerifyRow {
        id: col(row, 0)?,
        record: NewVerifyRecord {
            role: parsed(row, 1, "role", Role::parse)?,
            head_sha: col(row, 2)?,
            verdict: col(row, 3)?,
            exit_code: col(row, 4)?,
            seconds: seconds
                .map(|s| u64::try_from(s).map_err(|e| bad("seconds", e)))
                .transpose()?,
            flags: serde_json::from_str(&flags).map_err(|e| bad("flags", e))?,
            log_sha256: hash32(row, 7)?,
        },
        recorded_at: ts(row, 8)?,
    })
}

fn waiver_from_row(row: &Row<'_>) -> Result<Waiver, String> {
    Ok(Waiver {
        role: parsed(row, 0, "role", Role::parse)?,
        flag_id: col(row, 1)?,
        head_sha: col(row, 2)?,
        reason: col(row, 3)?,
        created_at: ts(row, 4)?,
    })
}

impl Tx<'_> {
    /// Stores a verify record and writes `verify.recorded`. Returns its id.
    pub fn insert_verify_record(
        &self,
        at: Timestamp,
        new: &NewVerifyRecord,
    ) -> Result<i64, StoreError> {
        self.connection().execute(
            "INSERT INTO verify_records (role, head_sha, verdict, exit_code, seconds, flags,
                                         log_sha256, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                new.role.as_str(),
                new.head_sha,
                new.verdict,
                new.exit_code,
                new.seconds.map(|s| i64::try_from(s).unwrap_or(i64::MAX)),
                json!(new.flags).to_string(),
                new.log_sha256.as_ref().map(<[u8; 32]>::as_slice),
                at.to_string(),
            ],
        )?;
        let id = self.connection().last_insert_rowid();
        self.record_event(
            at,
            EventType::VerifyRecorded,
            Some(&new.role),
            None,
            &json!({
                "verdict": new.verdict,
                "flags": new.flags,
                "exit": new.exit_code,
                "seconds": new.seconds,
            }),
        )?;
        Ok(id)
    }

    /// Stores a waiver and writes `verify.waived`. False, and no event, when
    /// the same flag on the same head was already waived.
    pub fn insert_waiver(&self, at: Timestamp, waiver: &Waiver) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "INSERT OR IGNORE INTO waivers (role, flag_id, head_sha, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                waiver.role.as_str(),
                waiver.flag_id,
                waiver.head_sha,
                waiver.reason,
                at.to_string(),
            ],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        self.record_event(
            at,
            EventType::VerifyWaived,
            Some(&waiver.role),
            None,
            &json!({ "flags": [waiver.flag_id], "head_sha": waiver.head_sha }),
        )?;
        Ok(true)
    }
}

impl ReadTx<'_> {
    /// Every message's id, body and stored body hash, oldest first. The rows
    /// are raw: unlike `messages`, a body that fails its hash is returned, so
    /// `verify` can find and repair it.
    pub fn message_bodies(&self) -> Result<Vec<(DisplayId, String, Vec<u8>)>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare("SELECT display_id, body, body_sha256 FROM messages ORDER BY rowid")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<(String, String, Vec<u8>)>, _>>()?;
        rows.into_iter()
            .map(|(id, body, sha)| {
                let display_id =
                    DisplayId::parse(&id).map_err(|e| StoreError::bad_value("messages", &id, e))?;
                Ok((display_id, body, sha))
            })
            .collect()
    }

    /// Every verify record of every role, oldest first (`teamwire export`).
    pub fn all_verify_records(&self) -> Result<Listing<VerifyRow>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {VERIFY_COLS} FROM verify_records ORDER BY id"
        ))?;
        listing("verify_records", &mut stmt, [], verify_from_row)
    }

    /// A member's verify records, oldest first.
    pub fn verify_records(&self, role: &Role) -> Result<Listing<VerifyRow>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {VERIFY_COLS} FROM verify_records WHERE role = ?1 ORDER BY id"
        ))?;
        listing(
            "verify_records",
            &mut stmt,
            [role.as_str()],
            verify_from_row,
        )
    }

    /// Every waiver, oldest first (`teamwire export`).
    pub fn all_waivers(&self) -> Result<Listing<Waiver>, StoreError> {
        let mut stmt = self.connection().prepare(
            "SELECT role, flag_id, head_sha, reason, created_at FROM waivers ORDER BY rowid",
        )?;
        listing("waivers", &mut stmt, [], waiver_from_row)
    }

    /// The waivers on `head_sha` for one member.
    pub fn waivers(&self, role: &Role, head_sha: &str) -> Result<Listing<Waiver>, StoreError> {
        let mut stmt = self.connection().prepare(
            "SELECT role, flag_id, head_sha, reason, created_at FROM waivers
             WHERE role = ?1 AND head_sha = ?2 ORDER BY rowid",
        )?;
        listing(
            "waivers",
            &mut stmt,
            [role.as_str(), head_sha],
            waiver_from_row,
        )
    }
}

#[cfg(test)]
mod tests;
