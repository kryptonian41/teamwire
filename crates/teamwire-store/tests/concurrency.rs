//! Two connections at once: readers never block or fail a writer, and a
//! writer waits for the lock instead of failing.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use teamwire_core::TeamName;
use teamwire_fakes::TestHome;
use teamwire_store::{Store, StoreError, Tx};

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

fn add_note(tx: &Tx<'_>, key: &str) -> Result<(), StoreError> {
    tx.connection().execute(
        "INSERT INTO announcements (key, at) VALUES (?1, 't')",
        [key],
    )?;
    Ok(())
}

fn count_notes(store: &Store) -> i64 {
    store
        .read(|tx| {
            Ok(tx
                .connection()
                .query_row("SELECT count(*) FROM announcements", [], |r| r.get(0))?)
        })
        .unwrap()
}

#[test]
fn read_snapshot_does_not_fail_writer() {
    let home = TestHome::new().unwrap();
    let reader = Store::open(home.path(), &team()).unwrap();
    let path = home.path().to_path_buf();

    let seen_inside = reader
        .read(|tx| {
            let count = |tx: &teamwire_store::ReadTx<'_>| -> Result<i64, StoreError> {
                Ok(tx
                    .connection()
                    .query_row("SELECT count(*) FROM announcements", [], |r| r.get(0))?)
            };
            let before = count(tx)?;
            // Another connection commits while this read transaction is open.
            let writer_path = path.clone();
            let outcome = thread::spawn(move || {
                let mut writer = Store::open(&writer_path, &team()).unwrap();
                writer.write(|tx| add_note(tx, "while-reading"))
            })
            .join()
            .unwrap();
            assert_eq!(outcome, Ok(()), "a reader made the writer fail");
            let after = count(tx)?;
            Ok((before, after))
        })
        .unwrap();
    assert_eq!(
        seen_inside,
        (0, 0),
        "the read transaction saw a later commit"
    );
    assert_eq!(count_notes(&reader), 1);
}

#[test]
fn a_writer_waits_for_the_lock_instead_of_failing() {
    let home = TestHome::new().unwrap();
    drop(Store::open(home.path(), &team()).unwrap());
    let (locked_tx, locked_rx) = mpsc::channel();
    let path = home.path().to_path_buf();
    let holder = thread::spawn(move || {
        let mut store = Store::open(&path, &team()).unwrap();
        store
            .write(|tx| {
                add_note(tx, "first")?;
                locked_tx.send(()).unwrap();
                thread::sleep(Duration::from_millis(400));
                Ok(())
            })
            .unwrap();
    });
    locked_rx.recv().unwrap();
    let mut second = Store::open(home.path(), &team()).unwrap();
    let started = Instant::now();
    second.write(|tx| add_note(tx, "second")).unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(200),
        "the second write did not wait"
    );
    holder.join().unwrap();
    assert_eq!(count_notes(&second), 2);
}

#[test]
fn a_failed_closure_rolls_back_everything_it_wrote() {
    let home = TestHome::new().unwrap();
    let mut store = Store::open(home.path(), &team()).unwrap();
    let result: Result<(), StoreError> = store.write(|tx| {
        add_note(tx, "kept-out")?;
        Err(StoreError::Io("stop".to_owned()))
    });
    assert_eq!(result, Err(StoreError::Io("stop".to_owned())));
    assert_eq!(count_notes(&store), 0);
    // The connection is usable afterwards.
    store.write(|tx| add_note(tx, "after")).unwrap();
    assert_eq!(count_notes(&store), 1);
}

#[test]
fn a_constraint_failure_is_reported_as_one() {
    let home = TestHome::new().unwrap();
    let mut store = Store::open(home.path(), &team()).unwrap();
    store.write(|tx| add_note(tx, "same")).unwrap();
    let again = store.write(|tx| add_note(tx, "same"));
    assert!(matches!(again, Err(StoreError::Constraint(_))), "{again:?}");
}

#[test]
fn backup_is_a_complete_readable_copy() {
    let home = TestHome::new().unwrap();
    let mut store = Store::open(home.path(), &team()).unwrap();
    store.write(|tx| add_note(tx, "in-backup")).unwrap();
    let target = home.path().join("copy.db");
    store.backup_to(&target).unwrap();
    let copy = rusqlite::Connection::open(&target).unwrap();
    let key: String = copy
        .query_row("SELECT key FROM announcements", [], |r| r.get(0))
        .unwrap();
    assert_eq!(key, "in-backup");
    let check: String = copy
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    assert!(
        store.backup_to(&target).is_err(),
        "a backup overwrote a file"
    );
    assert_eq!(store.integrity_check().unwrap(), "ok");
}
