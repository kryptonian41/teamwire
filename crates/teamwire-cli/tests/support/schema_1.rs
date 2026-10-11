//! A team database at schema 1, for the tests that read one unmigrated.
#![expect(
    clippy::unwrap_used,
    reason = "a test helper: a failure here fails the test"
)]

use std::path::{Path, PathBuf};

use teamwire_core::{TeamName, Timestamp, Ulid};
use teamwire_store::{SCHEMA_V1_SQL, Store, database_path};

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

/// A team database exactly as a schema-1 binary made it: every table from
/// `v1.sql`, `user_version` 1 and two watch lines. The test has
/// no `SQLite` library of its own, so a store connection empties a fresh
/// database and runs `v1.sql` in it.
pub(crate) fn version_1_team(home: &Path, team: &str, seed: u128) -> PathBuf {
    let name = TeamName::parse(team).unwrap();
    let ulid = Ulid::from_parts(at(0), seed);
    let (e5, e6) = (Ulid::from_parts(at(5), seed), Ulid::from_parts(at(6), seed));
    let mut store = Store::open(home, &name).unwrap();
    store
        .write(|tx| {
            let conn = tx.connection();
            let tables: Vec<String> = conn
                .prepare(
                    "SELECT name FROM sqlite_schema WHERE type = 'table'
                       AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
                )?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for table in tables {
                conn.execute_batch(&format!("DROP TABLE {table};"))?;
            }
            conn.execute_batch(SCHEMA_V1_SQL)?;
            conn.execute_batch(&format!(
                "INSERT INTO team (id, ulid, name, title, state, lead_harness, lead_host,
                                   lead_route, created_at)
                 VALUES (1, '{ulid}', '{team}', '{team}', 'open', 'claude-code', 'herdr', 'sink',
                         '{created}');
                 INSERT INTO events (ulid, time, type, data, writer_version) VALUES
                   ('{e5}', '{t5}', 'watch.line', '{{\"events\":[]}}', '0.1.0'),
                   ('{e6}', '{t6}', 'watch.line', '{{\"events\":[]}}', '0.1.0');
                 PRAGMA user_version = 1;",
                created = at(0),
                t5 = at(5),
                t6 = at(6),
            ))?;
            Ok(())
        })
        .unwrap();
    drop(store);
    database_path(home, &name)
}
