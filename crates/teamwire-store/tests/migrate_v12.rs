//! The step from schema version 11 to 12 rebuilds `events` so its closed
//! list of types holds the three dialog types (ADR 0052), on databases that
//! real teams already hold.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::Path;

use rusqlite::Connection;
use teamwire_core::TeamName;
use teamwire_fakes::TestHome;
use teamwire_store::{
    APPLICATION_ID, SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL, SCHEMA_V4_SQL, SCHEMA_V5_SQL,
    SCHEMA_V6_SQL, SCHEMA_V7_SQL, SCHEMA_V8_SQL, SCHEMA_V9_SQL, SCHEMA_V10_SQL, SCHEMA_V11_SQL,
    SCHEMA_VERSION, Store, database_path,
};

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

/// A version-11 database with a team row and three events.
fn version_11_database(home: &TestHome) -> std::path::PathBuf {
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
        SCHEMA_V8_SQL,
        SCHEMA_V9_SQL,
        SCHEMA_V10_SQL,
        SCHEMA_V11_SQL,
    ] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 11).unwrap();
    conn.execute_batch(
        "INSERT INTO team (id, ulid, name, title, state, lead_harness, lead_host, lead_route, created_at)
         VALUES (1, '01J9ZQ0000000000000000TEAM', 'alpha', 'Alpha', 'open', 'claude-code', 'herdr', 'sink',
                 '2026-11-01T00:00:00.000Z');
         INSERT INTO events (ulid, time, type, role, subject, data, writer_version) VALUES
           ('01J9ZQ000000000000000EVT01', '2026-11-01T00:00:01.000Z', 'team.created', NULL, NULL, '{}', '0.24.0'),
           ('01J9ZQ000000000000000EVT02', '2026-11-01T00:00:02.000Z', 'member.woken', 'impl', '01J9ZQ000000000000000MSG01', '{\"generation\":2}', '0.26.0'),
           ('01J9ZQ000000000000000EVT03', '2026-11-01T00:00:03.000Z', 'team.refs_added', NULL, NULL, '{}', '0.24.0');",
    )
    .unwrap();
    path
}

fn event_rows(path: &Path) -> Vec<(i64, String, String)> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT seq, ulid, type FROM events ORDER BY seq")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn a_version_11_store_migrates_and_takes_dialog_events() {
    let home = TestHome::new().unwrap();
    let path = version_11_database(&home);
    let before = event_rows(&path);
    assert_eq!(before.len(), 3);

    let store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(SCHEMA_VERSION, 12);
    assert_eq!(store.schema_version().unwrap(), 12);
    drop(store);
    assert_eq!(event_rows(&path), before, "every event kept with its seq");

    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "INSERT INTO events (ulid, time, type, role, subject, data, writer_version) VALUES
               ('01J9ZQ000000000000000EVT04', '2026-11-01T00:00:04.000Z', 'dialog.opened',
                'impl', NULL, '{\"tool\":\"Bash\"}', 'x'),
               ('01J9ZQ000000000000000EVT05', '2026-11-01T00:00:05.000Z', 'dialog.answered',
                'impl', '01J9ZQ000000000000000EVT04', '{\"verdict\":\"deny\"}', 'x'),
               ('01J9ZQ000000000000000EVT06', '2026-11-01T00:00:06.000Z', 'dialog.closed',
                'impl', '01J9ZQ000000000000000EVT04', '{\"outcome\":\"taken\"}', 'x')",
        )
        .unwrap();
    let after = event_rows(&path);
    assert_eq!(after.len(), 6);
    assert_eq!(
        after.last().unwrap().0,
        6,
        "the next seq follows the kept ones"
    );
}
