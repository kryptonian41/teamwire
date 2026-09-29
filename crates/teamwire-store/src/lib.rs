//! The team's `SQLite` database: opening it with the required settings,
//! migrating it, and one typed transaction method per operation.
//!
//! Never: hold a write transaction open during an external call (host,
//! harness, git, network); write outside `Store::write`; create or migrate
//! a database from `open_read_only`.
//!
//! Invariants: every write runs inside BEGIN IMMEDIATE; every `Tx` method
//! that changes a row also records its event row in the same transaction;
//! display ids and per-sender `seq` come from the `counters` table, never
//! from counting rows.
//!
//! This crate also holds `Ctx`, the handle every service call takes.

mod ctx;
mod error;
mod store;
mod tx;

pub use ctx::{Ctx, Party};
pub use error::StoreError;
pub use store::{APPLICATION_ID, BUSY_TIMEOUT_MS, Store, WRITER_VERSION, database_path};
pub use tx::{BadRow, Listing, ReadTx, Tx};

/// The schema version this binary writes.
pub const SCHEMA_VERSION: u32 = 1;

/// The DDL of schema version 1.
pub const SCHEMA_V1_SQL: &str = include_str!("schema/v1.sql");

#[cfg(test)]
mod tests {
    use super::*;
    use teamwire_core::{
        CompletionStatus, DeliveryState, EventType, FinalSource, MemberKind, MemberState,
        MessageType, Mode, ReportState, Route,
    };

    const TABLES: [&str; 12] = [
        "team",
        "members",
        "messages",
        "deliveries",
        "completions",
        "verify_records",
        "waivers",
        "watch_lease",
        "announcements",
        "counters",
        "events",
        "metric_export",
    ];

    fn schema() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(SCHEMA_V1_SQL).unwrap();
        conn
    }

    fn values<T: Copy>(all: &[T], as_str: fn(T) -> &'static str) -> Vec<&'static str> {
        all.iter().map(|v| as_str(*v)).collect()
    }

    #[test]
    fn ddl_creates_every_table() {
        let conn = schema();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let mut found: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        found.sort();
        let mut expected: Vec<String> = TABLES.iter().map(|t| (*t).to_owned()).collect();
        expected.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn ddl_has_the_integrity_indexes() {
        let conn = schema();
        for index in [
            "messages_sender_key",
            "messages_sender_seq",
            "messages_display",
            "completions_role_n",
        ] {
            let unique: i64 = conn
                .query_row(
                    "SELECT \"unique\" FROM pragma_index_list(?1) WHERE name = ?2",
                    [index.split('_').next().unwrap(), index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(unique, 1, "{index}");
        }
    }

    fn check_column(conn: &rusqlite::Connection, table: &str, column: &str, good: &[&str]) {
        // Each statement runs inside a savepoint that is rolled back, so the
        // rows other columns need are present and nothing stays behind.
        let insert = |value: &str| -> rusqlite::Result<usize> {
            conn.execute_batch("SAVEPOINT probe")?;
            let result = insert_row(conn, table, column, value);
            conn.execute_batch("ROLLBACK TO probe; RELEASE probe")?;
            result
        };
        for value in good {
            let result = insert(value);
            assert!(
                result.is_ok(),
                "{table}.{column} refused {value}: {result:?}"
            );
        }
        assert!(
            insert("no_such_value").is_err(),
            "{table}.{column} took a bad value"
        );
    }

    fn insert_row(
        conn: &rusqlite::Connection,
        table: &str,
        column: &str,
        value: &str,
    ) -> rusqlite::Result<usize> {
        let member = |role: &str, kind: &str, mode: &str, state: &str| {
            conn.execute(
                "INSERT INTO members (role, title, kind, mode, harness, host, cwd, deadline_s, state, generation, gen_started_at)
                 VALUES (?1, 't', ?2, ?3, 'claude-code', 'herdr', '/x', 1800, ?4, 1, 't0')",
                [role, kind, mode, state],
            )
        };
        let message = |ulid: &str, kind: &str| {
            conn.execute(
                "INSERT INTO messages (ulid, display_id, sender, seq, type, body, body_sha256, created_at)
                 VALUES (?1, ?1, 'lead', 1, ?2, 'b', zeroblob(32), 't0')",
                [ulid, kind],
            )
        };
        match (table, column) {
            ("members", "kind") => member("a", value, "pane", "started"),
            ("members", "mode") => member("a", "reviewer", value, "started"),
            ("members", "state") => member("a", "reviewer", "pane", value),
            ("messages", "type") => message("m1", value),
            ("deliveries", column) => {
                message("m1", "note")?;
                let (state, route) = if column == "state" { (value, "inbox") } else { ("pending", value) };
                conn.execute(
                    "INSERT INTO deliveries (message_ulid, recipient, state, route) VALUES ('m1', 'a', ?1, ?2)",
                    [state, route],
                )
            }
            ("completions", column) => {
                member("a", "reviewer", "pane", "started")?;
                let mut v = ["done", "none", "pending", "inbox"];
                let slot = ["status", "final_source", "report_state", "route"]
                    .iter()
                    .position(|c| *c == column)
                    .unwrap();
                if let Some(field) = v.get_mut(slot) {
                    *field = value;
                }
                conn.execute(
                    "INSERT INTO completions (ulid, role, generation, n, status, summary, final_source, report_state, route, created_at)
                     VALUES ('c1', 'a', 1, 1, ?1, 's', ?2, ?3, ?4, 't0')",
                    v,
                )
            }
            ("events", "type") => conn.execute(
                "INSERT INTO events (ulid, time, type, writer_version) VALUES ('e1', 't0', ?1, '0.0.0')",
                [value],
            ),
            _ => Err(rusqlite::Error::InvalidColumnName(format!("{table}.{column}"))),
        }
    }

    #[test]
    fn enum_columns_take_exactly_the_core_values() {
        let conn = schema();
        check_column(
            &conn,
            "members",
            "kind",
            &values(MemberKind::ALL, MemberKind::as_str),
        );
        check_column(&conn, "members", "mode", &values(Mode::ALL, Mode::as_str));
        check_column(
            &conn,
            "members",
            "state",
            &values(MemberState::ALL, MemberState::as_str),
        );
        check_column(
            &conn,
            "messages",
            "type",
            &values(MessageType::ALL, MessageType::as_str),
        );
        check_column(
            &conn,
            "deliveries",
            "state",
            &values(DeliveryState::ALL, DeliveryState::as_str),
        );
        check_column(
            &conn,
            "deliveries",
            "route",
            &values(Route::ALL, Route::as_str),
        );
        check_column(
            &conn,
            "completions",
            "status",
            &values(CompletionStatus::ALL, CompletionStatus::as_str),
        );
        check_column(
            &conn,
            "completions",
            "final_source",
            &values(FinalSource::ALL, FinalSource::as_str),
        );
        check_column(
            &conn,
            "completions",
            "report_state",
            &values(ReportState::ALL, ReportState::as_str),
        );
        check_column(
            &conn,
            "completions",
            "route",
            &values(Route::ALL, Route::as_str),
        );
        check_column(
            &conn,
            "events",
            "type",
            &values(EventType::ALL, EventType::as_str),
        );
    }

    #[test]
    fn delivery_needs_its_message() {
        let conn = schema();
        let orphan = conn.execute(
            "INSERT INTO deliveries (message_ulid, recipient, state) VALUES ('nope', 'a', 'pending')",
            [],
        );
        assert!(orphan.is_err());
    }
}
