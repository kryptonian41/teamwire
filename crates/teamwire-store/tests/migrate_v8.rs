//! The step from schema version 7 to 8 rebuilds `events` a fourth time, for
//! `delivery.recipient_stopped`, on databases that real teams already hold.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::Path;

use rusqlite::Connection;
use teamwire_core::{EventType, TeamName, Timestamp};
use teamwire_fakes::TestHome;
use teamwire_store::{
    APPLICATION_ID, SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL, SCHEMA_V4_SQL, SCHEMA_V5_SQL,
    SCHEMA_V6_SQL, SCHEMA_V7_SQL, SCHEMA_VERSION, Store, database_path,
};

type Row = (
    i64,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

/// A version-7 database whose newest two events were deleted, so the
/// `AUTOINCREMENT` counter (5) is higher than the highest `seq` kept (3).
fn version_7_database(home: &TestHome) -> std::path::PathBuf {
    let path = database_path(home.path(), &team());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    for step in [
        SCHEMA_V1_SQL,
        SCHEMA_V2_SQL,
        SCHEMA_V3_SQL,
        SCHEMA_V4_SQL,
        SCHEMA_V5_SQL,
        SCHEMA_V6_SQL,
        SCHEMA_V7_SQL,
    ] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 7).unwrap();
    conn.execute_batch(
        "INSERT INTO events (ulid, time, type, role, subject, data, writer_version) VALUES
           ('01J9ZQ000000000000000EVT01', '2026-10-01T00:00:01.000Z', 'team.created', NULL, NULL, '{}', '0.3.0'),
           ('01J9ZQ000000000000000EVT02', '2026-10-01T00:00:02.000Z', 'member.handoff', 'impl', '01J9ZQ000000000000000MSG01', '{\"attempt\":2}', '0.3.0'),
           ('01J9ZQ000000000000000EVT03', '2026-10-01T00:00:03.000Z', 'store.error', NULL, NULL, '{\"why\":\"x\"}', '0.3.1'),
           ('01J9ZQ000000000000000EVT04', '2026-10-01T00:00:04.000Z', 'watch.started', NULL, NULL, '{}', '0.3.1'),
           ('01J9ZQ000000000000000EVT05', '2026-10-01T00:00:05.000Z', 'watch.stopped', NULL, NULL, '{}', '0.3.1');
         DELETE FROM events WHERE seq > 3;
         INSERT INTO metric_export (id, last_event_seq) VALUES (1, 5);",
    )
    .unwrap();
    path
}

fn event_rows(path: &Path) -> Vec<Row> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT seq, ulid, time, type, role, subject, data, writer_version FROM events ORDER BY seq")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
            r.get(7)?,
        ))
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

const NEW_EVENT: &str = "INSERT INTO events (ulid, time, type, writer_version)
    VALUES ('01J9ZQ000000000000000EVT09', '2026-10-01T00:00:09.000Z', 'delivery.recipient_stopped', 'x')";

#[test]
fn the_new_type_is_refused_before_the_step_and_accepted_after() {
    let home = TestHome::new().unwrap();
    let path = version_7_database(&home);
    let before = Connection::open(&path).unwrap().execute_batch(NEW_EVENT);
    assert!(before.is_err(), "a version-7 table accepted the new type");

    drop(Store::open(home.path(), &team()).unwrap());
    Connection::open(&path)
        .unwrap()
        .execute_batch(NEW_EVENT)
        .unwrap();
}

#[test]
fn a_version_7_database_keeps_every_event_and_never_reuses_a_seq() {
    let home = TestHome::new().unwrap();
    let path = version_7_database(&home);
    let before = event_rows(&path);
    assert_eq!(before.len(), 3);

    let mut store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(event_rows(&path), before);

    // The counter stood at 5 when the migration ran: the next event is 6, not 4.
    let at = Timestamp::from_unix_millis(1_790_000_000_000).unwrap();
    store
        .write(|tx| {
            tx.record_event(
                at,
                EventType::DeliveryRecipientStopped,
                None,
                None,
                &serde_json::json!({}),
            )
            .map(|_| ())
        })
        .unwrap();
    let after = event_rows(&path);
    assert_eq!(after.last().unwrap().0, 6);
    assert_eq!(after.last().unwrap().3, "delivery.recipient_stopped");

    let conn = Connection::open(&path).unwrap();
    let indexes: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name = 'events_type' AND tbl_name = 'events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 1);
    let leftover: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'events\\_old%' ESCAPE '\\'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leftover, 0);
}

/// An empty `events` table has no counter row; the step must not need one.
#[test]
fn a_version_7_database_with_no_events_migrates() {
    let home = TestHome::new().unwrap();
    let path = version_7_database(&home);
    Connection::open(&path)
        .unwrap()
        .execute_batch("DELETE FROM events; DELETE FROM sqlite_sequence;")
        .unwrap();
    drop(Store::open(home.path(), &team()).unwrap());
    assert!(event_rows(&path).is_empty());
}

#[test]
fn a_read_only_open_reads_a_version_7_database_and_writes_nothing() {
    let home = TestHome::new().unwrap();
    let path = version_7_database(&home);
    let bytes = std::fs::read(&path).unwrap();

    let store = Store::open_read_only(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 7);
    let events = store.read(|tx| tx.events_after(0, None, None)).unwrap();
    assert_eq!(events.items.len(), 3);
    drop(store);

    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

/// The step fails after it has renamed and rebuilt the table: one stored row
/// has a type that no list holds. The open must fail and leave version 7 with
/// all its rows, as a crash at that point would.
#[test]
fn a_step_that_fails_midway_leaves_the_version_7_database_whole() {
    let home = TestHome::new().unwrap();
    let path = version_7_database(&home);
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO events (ulid, time, type, writer_version)
             VALUES ('01J9ZQ000000000000000EVT07', '2026-10-01T00:00:07.000Z', 'not.a.type', 'x');",
        )
        .unwrap();
    }
    let before = event_rows(&path);
    assert_eq!(before.len(), 4);

    assert!(Store::open(home.path(), &team()).is_err());

    assert_eq!(event_rows(&path), before);
    let conn = Connection::open(&path).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 7);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name = 'events_old'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
}
