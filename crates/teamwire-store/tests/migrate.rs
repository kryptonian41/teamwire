//! Opening, pragmas, migration and the refusal of files that are not ours.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::io::Write as _;
use std::path::PathBuf;

use rusqlite::Connection;
use teamwire_core::{EventType, Pid, TeamName, TeamState};
use teamwire_fakes::TestHome;
use teamwire_store::{
    APPLICATION_ID, SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL, SCHEMA_V4_SQL, SCHEMA_V5_SQL,
    SCHEMA_VERSION, Store, StoreError, database_path,
};

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

fn pragma_int(store: &Store, name: &str) -> i64 {
    store
        .read(|tx| {
            Ok(tx
                .connection()
                .query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))?)
        })
        .unwrap()
}

fn journal_mode(store: &Store) -> String {
    store
        .read(|tx| {
            Ok(tx
                .connection()
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))?)
        })
        .unwrap()
}

/// Every table and index, as the text `SQLite` keeps: two databases with the
/// same list have the same schema.
fn schema_dump(path: &PathBuf) -> Vec<String> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT type || ' ' || name || ' ' || coalesce(sql, '') FROM sqlite_schema ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn file_bytes(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn every_connection_has_the_required_pragmas() {
    let home = TestHome::new().unwrap();
    let store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(journal_mode(&store), "wal");
    assert_eq!(pragma_int(&store, "synchronous"), 2); // FULL
    assert_eq!(pragma_int(&store, "foreign_keys"), 1);
    let busy = pragma_int(&store, "busy_timeout");
    assert!(busy >= 5000, "busy_timeout is {busy}");
    assert_eq!(
        pragma_int(&store, "application_id"),
        i64::from(APPLICATION_ID)
    );
    assert_eq!(
        pragma_int(&store, "user_version"),
        i64::from(SCHEMA_VERSION)
    );
    // A second open of the same file gets the same settings.
    drop(store);
    let again = Store::open_existing(home.path(), &team()).unwrap();
    assert_eq!(pragma_int(&again, "synchronous"), 2);
    assert_eq!(pragma_int(&again, "foreign_keys"), 1);
    assert_eq!(again.schema_version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn open_creates_directory_and_open_existing_never_does() {
    let home = TestHome::new().unwrap();
    let missing = Store::open_existing(home.path(), &team());
    assert!(
        matches!(missing, Err(StoreError::NoDatabase { .. })),
        "{missing:?}"
    );
    assert!(!home.path().join("teams").exists());
    Store::open(home.path(), &team()).unwrap();
    assert!(database_path(home.path(), &team()).is_file());
}

#[test]
fn newer_schema_refused_unchanged() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    drop(Store::open(home.path(), &team()).unwrap());
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 4)
            .unwrap();
    }
    let before = file_bytes(&path);
    let refused = Store::open(home.path(), &team()).unwrap_err();
    assert_eq!(
        refused,
        StoreError::SchemaNewer {
            db: SCHEMA_VERSION + 4,
            binary: SCHEMA_VERSION
        }
    );
    let existing = Store::open_existing(home.path(), &team()).unwrap_err();
    assert!(matches!(existing, StoreError::SchemaNewer { .. }));
    assert_eq!(
        file_bytes(&path),
        before,
        "the refused open changed the file"
    );
    let wal = path.with_extension("db-wal");
    assert!(!wal.exists() || std::fs::metadata(wal).unwrap().len() == 0);
}

#[test]
fn read_only_open_never_migrates() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());

    // No file: refused, and nothing is created.
    let missing = Store::open_read_only(home.path(), &team());
    assert!(
        matches!(missing, Err(StoreError::NoDatabase { .. })),
        "{missing:?}"
    );
    assert!(!path.exists());

    // An empty database (no schema): refused as older, and stays empty.
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    drop(Connection::open(&path).unwrap());
    let empty = Store::open_read_only(home.path(), &team()).unwrap_err();
    assert_eq!(
        empty,
        StoreError::SchemaOlder {
            db: 0,
            binary: SCHEMA_VERSION
        }
    );
    let conn = Connection::open(&path).unwrap();
    let tables: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tables, 0);
    drop(conn);
    std::fs::remove_file(&path).unwrap();

    // A current database opens, reads, and refuses to write.
    drop(Store::open(home.path(), &team()).unwrap());
    let mut ro = Store::open_read_only(home.path(), &team()).unwrap();
    assert_eq!(ro.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(ro.quick_check().unwrap(), "ok");
    assert!(ro.write(|_| Ok(())).is_err());
    let write_through_read = ro.read(|tx| {
        tx.connection()
            .execute("INSERT INTO announcements (key, at) VALUES ('k', 't')", [])?;
        Ok(())
    });
    assert!(write_through_read.is_err());
    drop(ro);

    // Older and newer versions are refused, with the file byte-for-byte kept.
    for version in [SCHEMA_VERSION + 1, 0] {
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        let before = file_bytes(&path);
        let refused = Store::open_read_only(home.path(), &team()).unwrap_err();
        let expected = if version > SCHEMA_VERSION {
            StoreError::SchemaNewer {
                db: version,
                binary: SCHEMA_VERSION,
            }
        } else {
            // version 0 with tables and our id is not a database we made.
            StoreError::NotTeamwire
        };
        assert_eq!(refused, expected);
        assert_eq!(file_bytes(&path), before);
    }
}

#[test]
fn foreign_database_refused_untouched() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE notes (x); INSERT INTO notes VALUES (1);")
            .unwrap();
    }
    let before = file_bytes(&path);
    assert_eq!(
        Store::open(home.path(), &team()).unwrap_err(),
        StoreError::NotTeamwire
    );
    assert_eq!(
        Store::open_read_only(home.path(), &team()).unwrap_err(),
        StoreError::NotTeamwire
    );
    assert_eq!(file_bytes(&path), before);

    // A file that is not SQLite at all.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    file.write_all(&b"this is not a database, just some text padded out\n".repeat(40))
        .unwrap();
    drop(file);
    assert_eq!(
        Store::open(home.path(), &team()).unwrap_err(),
        StoreError::NotTeamwire
    );
}

#[test]
fn writer_version_recorded() {
    let home = TestHome::new().unwrap();
    let at = teamwire_core::Timestamp::from_unix_millis(1_790_000_000_000).unwrap();
    let mut old = Store::open(home.path(), &team())
        .unwrap()
        .with_writer_version("0.1.0");
    let mut new = Store::open(home.path(), &team())
        .unwrap()
        .with_writer_version("0.2.0");
    for store in [&mut old, &mut new] {
        store
            .write(|tx| {
                tx.record_event(
                    at,
                    EventType::WatchStarted,
                    None,
                    None,
                    &serde_json::json!({}),
                )
                .map(|_| ())
            })
            .unwrap();
    }
    let versions: Vec<String> = old
        .read(|tx| {
            let mut stmt = tx
                .connection()
                .prepare("SELECT writer_version FROM events ORDER BY seq")?;
            Ok(stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?)
        })
        .unwrap();
    assert_eq!(versions, ["0.1.0", "0.2.0"]);
}

#[test]
fn migration_from_every_older_version_ends_in_the_same_schema() {
    // Version 0 is an empty database; the fresh open is the reference.
    let reference_home = TestHome::new().unwrap();
    drop(Store::open(reference_home.path(), &team()).unwrap());
    let reference = schema_dump(&database_path(reference_home.path(), &team()));
    assert!(
        reference
            .iter()
            .any(|l| l.contains("CREATE TABLE messages"))
    );

    for start in 0..SCHEMA_VERSION {
        let home = TestHome::new().unwrap();
        let path = database_path(home.path(), &team());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        {
            // Build the database as version `start` left it.
            let conn = Connection::open(&path).unwrap();
            if start >= 1 {
                for step in STEPS.iter().take(start as usize) {
                    conn.execute_batch(step).unwrap();
                }
                conn.pragma_update(None, "application_id", APPLICATION_ID)
                    .unwrap();
                conn.pragma_update(None, "user_version", start).unwrap();
            }
        }
        let store = Store::open(home.path(), &team()).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        drop(store);
        assert_eq!(schema_dump(&path), reference, "from version {start}");
    }
}

/// A version-1 database, as a version-1 binary left it mid-close and
/// mid-export, migrates to the current version with every row kept. The new
/// start-time columns are empty, so the pid alone still names each owner.
#[test]
fn a_version_1_database_with_rows_migrates_and_keeps_them() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA_V1_SQL).unwrap();
        conn.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(
            "INSERT INTO team (id, ulid, name, title, state, closing_pid, lead_harness, lead_host,
                               lead_route, created_at)
             VALUES (1, '01J9ZQ0000000000000000TEAM', 'alpha', 'Alpha', 'closing', 4242,
                     'claude-code', 'herdr', 'sink', '2026-10-01T00:00:00.000Z');
             INSERT INTO events (ulid, time, type, writer_version)
             VALUES ('01J9ZQ000000000000000EVENT', '2026-10-01T00:00:01.000Z', 'team.created', '0.1.0');
             INSERT INTO metric_export (id, last_event_seq, claimed_to, claimed_by, claimed_at)
             VALUES (1, 0, 1, '5151', '2026-10-01T00:00:02.000Z');",
        )
        .unwrap();
    }
    let store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    let (row, mark, events) = store
        .read(|tx| {
            Ok((
                tx.team()?.unwrap(),
                tx.export_mark()?,
                tx.events_after(0, None, None)?,
            ))
        })
        .unwrap();
    assert_eq!(row.name.as_str(), "alpha");
    assert_eq!(row.state, TeamState::Closing);
    assert_eq!(row.closing_pid, Some(Pid::new(4242)));
    assert_eq!(row.closing_started, None);
    assert_eq!(
        (mark.claimed_to, mark.claimed_by.as_deref()),
        (Some(1), Some("5151"))
    );
    assert_eq!(mark.claimed_by_started, None);
    assert_eq!(events.items.len(), 1);
    assert!(events.bad.is_empty(), "{:?}", events.bad);
}

/// Every file in the team directory with a hash of its bytes.
fn directory_state(dir: &std::path::Path) -> Vec<(String, u64, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            let bytes = std::fs::read(e.path()).unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                bytes.len() as u64,
                bytes,
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn read_only_open_creates_changes_and_deletes_no_file() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    let dir = path.parent().unwrap().to_path_buf();
    // The last connection to close checkpoints the WAL and removes -wal and
    // -shm, leaving only team.db: the case a status run meets on a quiet team.
    drop(Store::open(home.path(), &team()).unwrap());
    let names: Vec<String> = directory_state(&dir).into_iter().map(|f| f.0).collect();
    assert_eq!(names, ["team.db"], "the setup left WAL side files behind");
    let before = directory_state(&dir);

    let ro = Store::open_read_only(home.path(), &team()).unwrap();
    assert_eq!(ro.quick_check().unwrap(), "ok");
    ro.read(|tx| tx.members().map(|_| ())).unwrap();
    drop(ro);
    assert_eq!(
        directory_state(&dir),
        before,
        "closing a read-only store changed the directory"
    );

    // The same by path, as archives are opened.
    let by_path = Store::open_read_only_at(&path).unwrap();
    by_path
        .read(|tx| tx.events_after(0, None, None).map(|_| ()))
        .unwrap();
    drop(by_path);
    assert_eq!(directory_state(&dir), before);
}

#[test]
fn closing_a_read_only_store_leaves_a_live_writers_files_alone() {
    let home = TestHome::new().unwrap();
    let mut writer = Store::open(home.path(), &team()).unwrap();
    let dir = database_path(home.path(), &team())
        .parent()
        .unwrap()
        .to_path_buf();
    let names = |dir: &std::path::Path| -> Vec<String> {
        directory_state(dir).into_iter().map(|f| f.0).collect()
    };
    let with_writer = names(&dir);
    assert!(
        with_writer.contains(&"team.db-wal".to_owned()),
        "{with_writer:?}"
    );
    drop(Store::open_read_only(home.path(), &team()).unwrap());
    assert_eq!(names(&dir), with_writer);
    writer
        .write(|tx| {
            tx.connection()
                .execute("INSERT INTO announcements (key, at) VALUES ('k', 't')", [])?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn open_existing_missing_is_no_database() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    let missing = Store::open_existing(home.path(), &team());
    assert!(
        matches!(&missing, Err(StoreError::NoDatabase { path: named }) if *named == path),
        "{missing:?}"
    );
    // A file that is there but is not ours is another failure, never
    // "no database".
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE other (x INTEGER);")
            .unwrap();
    }
    let foreign = Store::open_existing(home.path(), &team());
    assert!(
        !matches!(foreign, Err(StoreError::NoDatabase { .. })),
        "{foreign:?}"
    );
    assert!(foreign.is_err(), "a foreign database must not open");
}

#[test]
fn a_refused_read_only_open_leaves_no_wal_or_shm_file() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    let dir = path.parent().unwrap().to_path_buf();
    drop(Store::open(home.path(), &team()).unwrap());
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    let before = directory_state(&dir);
    let names: Vec<&str> = before.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["team.db"], "the setup left WAL side files behind");
    assert!(matches!(
        Store::open_read_only(home.path(), &team()),
        Err(StoreError::SchemaNewer { .. })
    ));
    assert_eq!(
        directory_state(&dir),
        before,
        "a refused read-only open changed the directory"
    );
}

#[test]
fn a_table_named_like_sqlite_underscore_is_not_an_internal_table() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE sqlitex (x);").unwrap();
    }
    assert_eq!(
        Store::open_read_only(home.path(), &team()).unwrap_err(),
        StoreError::NotTeamwire
    );
}

/// Builds a version-1 database, as a version-1 binary left it mid-close and
/// mid-export, with the WAL folded in so `team.db` is the only file.
fn version_1_database(path: &PathBuf) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(SCHEMA_V1_SQL).unwrap();
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.execute_batch(
        "INSERT INTO team (id, ulid, name, title, state, closing_pid, lead_harness, lead_host,
                           lead_route, created_at)
         VALUES (1, '01J9ZQ0000000000000000TEAM', 'alpha', 'Alpha', 'closing', 4242,
                 'claude-code', 'herdr', 'sink', '2026-10-01T00:00:00.000Z');
         INSERT INTO events (ulid, time, type, writer_version)
         VALUES ('01J9ZQ000000000000000EVENT', '2026-10-01T00:00:01.000Z', 'team.created', '0.1.0');
         INSERT INTO metric_export (id, last_event_seq, claimed_to, claimed_by, claimed_at)
         VALUES (1, 0, 1, '5151', '2026-10-01T00:00:02.000Z');",
    )
    .unwrap();
}

/// A live team no read-write open has touched since an upgrade, and every
/// archive, sit at an older schema. A read-only open reads them as they are,
/// with the columns a later version added read as NULL, and writes nothing
/// (WAT-11).
#[test]
fn a_read_only_open_reads_a_version_1_database_and_changes_nothing() {
    let home = TestHome::new().unwrap();
    let path = database_path(home.path(), &team());
    version_1_database(&path);
    let dir = path.parent().unwrap().to_path_buf();
    let before = directory_state(&dir);
    let names: Vec<&str> = before.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["team.db"]);

    for store in [
        Store::open_read_only(home.path(), &team()).unwrap(),
        Store::open_read_only_at(&path).unwrap(),
    ] {
        assert_eq!(store.schema_version().unwrap(), 1);
        let (row, mark, events) = store
            .read(|tx| {
                Ok((
                    tx.team()?.unwrap(),
                    tx.export_mark()?,
                    tx.events_after(0, None, None)?,
                ))
            })
            .unwrap();
        assert_eq!(row.closing_pid, Some(Pid::new(4242)));
        assert_eq!(row.closing_started, None);
        assert_eq!(mark.claimed_by.as_deref(), Some("5151"));
        assert_eq!(mark.claimed_by_started, None);
        assert_eq!(events.items.len(), 1);
        drop(store);
        assert_eq!(
            directory_state(&dir),
            before,
            "the read changed the directory"
        );
    }
    let conn = Connection::open(&path).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 1);
}

/// Every migration step, so that a new schema version must be added here.
const STEPS: [&str; SCHEMA_VERSION as usize] = [
    SCHEMA_V1_SQL,
    SCHEMA_V2_SQL,
    SCHEMA_V3_SQL,
    SCHEMA_V4_SQL,
    SCHEMA_V5_SQL,
];

/// Every column a read sees on a database migrated to the current version,
/// a read-only open of each older version sees too. A migration that adds a
/// column without listing it for unmigrated reads fails here.
#[test]
fn a_read_only_open_of_every_older_version_shows_every_current_column() {
    let reference_home = TestHome::new().unwrap();
    drop(Store::open(reference_home.path(), &team()).unwrap());
    let columns = |store: &Store| -> Vec<(String, Vec<String>)> {
        store
            .read(|tx| {
                let conn = tx.connection();
                let mut tables = conn.prepare(
                    "SELECT name FROM main.sqlite_schema WHERE type = 'table'
                       AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
                )?;
                let names: Vec<String> = tables
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<_, _>>()?;
                let mut out = Vec::new();
                for name in names {
                    let stmt = conn.prepare(&format!("SELECT * FROM {name} LIMIT 0"))?;
                    let mut cols: Vec<String> =
                        stmt.column_names().into_iter().map(str::to_owned).collect();
                    cols.sort();
                    out.push((name, cols));
                }
                Ok(out)
            })
            .unwrap()
    };
    let reference = columns(&Store::open_read_only(reference_home.path(), &team()).unwrap());
    assert!(reference.iter().any(|(t, _)| t == "metric_export"));

    for start in 1..SCHEMA_VERSION {
        let home = TestHome::new().unwrap();
        let path = database_path(home.path(), &team());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        {
            // Build the database as version `start` left it.
            let conn = Connection::open(&path).unwrap();
            for step in STEPS.iter().take(start as usize) {
                conn.execute_batch(step).unwrap();
            }
            conn.pragma_update(None, "application_id", APPLICATION_ID)
                .unwrap();
            conn.pragma_update(None, "user_version", start).unwrap();
        }
        let store = Store::open_read_only(home.path(), &team()).unwrap();
        assert_eq!(store.schema_version().unwrap(), start);
        assert_eq!(columns(&store), reference, "from version {start}");
    }
}
