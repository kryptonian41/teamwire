//! A full disk: a low `max_page_count` makes `SQLite` answer `SQLITE_FULL` the
//! way a full volume does.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use teamwire_core::{EventType, TeamName, Timestamp};
use teamwire_fakes::TestHome;
use teamwire_store::{Store, StoreError};

fn set_max_pages(store: &Store, pages: i64) {
    store
        .read(|tx| {
            tx.connection()
                .query_row(&format!("PRAGMA max_page_count = {pages}"), [], |r| {
                    r.get::<_, i64>(0)
                })?;
            Ok(())
        })
        .unwrap();
}

fn notes(store: &Store) -> Vec<String> {
    store
        .read(|tx| {
            let mut stmt = tx
                .connection()
                .prepare("SELECT key FROM announcements ORDER BY key")?;
            Ok(stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?)
        })
        .unwrap()
}

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

fn store_errors(store: &Store) -> Vec<serde_json::Value> {
    store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventType::StoreError)
        .map(|e| e.data)
        .collect()
}

#[test]
fn a_full_disk_rolls_back_and_names_the_file() {
    let home = TestHome::new().unwrap();
    let mut store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
    store
        .write(|tx| {
            tx.connection().execute(
                "INSERT INTO announcements (key, at) VALUES ('before', 't')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let pages: i64 = store
        .read(|tx| {
            Ok(tx
                .connection()
                .query_row("PRAGMA page_count", [], |r| r.get(0))?)
        })
        .unwrap();
    set_max_pages(&store, pages + 4);

    let big = "x".repeat(64 * 1024);
    let full = store.write(|tx| {
        tx.connection().execute(
            "INSERT INTO announcements (key, at) VALUES ('small', 't')",
            [],
        )?;
        for i in 0..64 {
            tx.connection().execute(
                "INSERT INTO announcements (key, at) VALUES (?1, ?2)",
                [format!("big{i}"), big.clone()],
            )?;
        }
        Ok(())
    });
    // SQLite names the file by its resolved path (/private/var on macOS).
    let path = std::fs::canonicalize(teamwire_store::database_path(
        home.path(),
        &TeamName::parse("alpha").unwrap(),
    ))
    .unwrap();
    assert_eq!(full, Err(StoreError::Full { path: path.clone() }));
    let message = full.unwrap_err().to_string();
    assert!(message.contains(&path.display().to_string()), "{message}");
    assert_eq!(
        notes(&store),
        ["before"],
        "the failed transaction left rows"
    );
    assert_eq!(store.integrity_check().unwrap(), "ok");

    // The row about the failure waits for room: the second transaction that
    // writes it fails the same way while the disk stays full. Smaller event
    // rows fill the pages the rollback left free first.
    let mut filled = false;
    for i in 0..10_000 {
        let small = store.write(|tx| {
            tx.record_event(
                at(i),
                EventType::WatchLine,
                None,
                None,
                &serde_json::json!({}),
            )
        });
        if small.is_err() {
            filled = true;
            break;
        }
    }
    assert!(filled, "the disk never filled");
    store.record_failure(at(1), "send");
    assert_eq!(store_errors(&store), Vec::<serde_json::Value>::new());

    // With room again the same connection writes normally.
    set_max_pages(&store, pages + 1000);
    store.record_failure(at(2), "send");
    assert_eq!(
        store_errors(&store),
        [serde_json::json!({ "op": "send", "code": "full" })]
    );
    store
        .write(|tx| {
            tx.connection().execute(
                "INSERT INTO announcements (key, at) VALUES ('after', 't')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(notes(&store), ["after", "before"]);
}
