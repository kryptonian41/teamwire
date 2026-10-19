//! Reading a database at an older schema without migrating it. A read-only
//! open may meet a live team that no read-write open has touched since an
//! upgrade, or an archive, which nothing ever opens read-write. Every column a
//! later version added is shown as NULL through a temporary view of the same
//! name, so each reader runs its usual query. The views live in the
//! connection's temp schema, which `SQLite` searches before `main`; the file
//! is never written.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::SCHEMA_VERSION;
use crate::error::StoreError;

/// The columns each migration adds, as (table, column): `ADDED_COLUMNS[i]`
/// belongs to the step from version `i` to `i + 1`. Every added column is
/// nullable or has a default the reader accepts NULL for (`team.refs` reads
/// NULL as no refs). A step that does more than add such a column needs
/// another way to be read unmigrated.
const ADDED_COLUMNS: [&[(&str, &str)]; SCHEMA_VERSION as usize] = [
    &[],
    &[
        ("team", "closing_pid_started_at"),
        ("metric_export", "claimed_by_started_at"),
    ],
    &[],
    &[],
    &[("team", "afk_budget_ends_at")],
    &[("team", "refs")],
];

/// Shows the columns that a database at version `db` lacks as NULL, on a
/// connection that has not yet been made `query_only`.
pub(crate) fn show_missing_columns(conn: &Connection, db: u32) -> Result<(), StoreError> {
    let mut missing: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for step in ADDED_COLUMNS.iter().skip(db as usize) {
        for (table, column) in *step {
            missing.entry(table).or_default().push(column);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    // Keeps the temp schema off the disk: no file appears anywhere.
    conn.execute_batch("PRAGMA temp_store = MEMORY;")?;
    for (table, columns) in missing {
        let nulls: String = columns
            .iter()
            .map(|c| format!(", NULL AS {c}"))
            .collect::<Vec<_>>()
            .concat();
        conn.execute_batch(&format!(
            "CREATE TEMP VIEW {table} AS SELECT *{nulls} FROM main.{table};"
        ))?;
    }
    Ok(())
}
