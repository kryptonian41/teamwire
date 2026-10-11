//! Two connections at once: readers never block or fail a writer, and a
//! writer waits for the lock instead of failing.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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
    let holder_committed = Arc::new(AtomicBool::new(false));
    let path = home.path().to_path_buf();
    let flag = Arc::clone(&holder_committed);
    let holder = thread::spawn(move || {
        let mut store = Store::open(&path, &team()).unwrap();
        store
            .write(|tx| {
                add_note(tx, "first")?;
                locked_tx.send(()).unwrap();
                thread::sleep(Duration::from_millis(400));
                // Set just before the commit: the second writer can only run
                // its closure after that commit released the lock.
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
    });
    locked_rx.recv().unwrap();
    let mut second = Store::open(home.path(), &team()).unwrap();
    let ran_after_holder = second
        .write(|tx| {
            add_note(tx, "second")?;
            Ok(holder_committed.load(Ordering::SeqCst))
        })
        .unwrap();
    assert!(
        ran_after_holder,
        "the second write did not wait for the first"
    );
    holder.join().unwrap();
    assert_eq!(count_notes(&second), 2);
}

/// A write transaction that first reads and then writes must not fail when
/// another connection wants to write in between. Under a deferred BEGIN the
/// read takes a snapshot, the other connection commits, and the first write
/// fails with `SQLITE_BUSY_SNAPSHOT`. BEGIN IMMEDIATE takes the write lock
/// before the read, so the other connection waits instead.
#[test]
fn a_read_then_write_transaction_is_not_overtaken_by_another_writer() {
    let home = TestHome::new().unwrap();
    let mut first = Store::open(home.path(), &team()).unwrap();
    let path = home.path().to_path_buf();

    let other = first
        .write(|tx| {
            let before: i64 =
                tx.connection()
                    .query_row("SELECT count(*) FROM announcements", [], |r| r.get(0))?;
            assert_eq!(before, 0);
            let other = thread::spawn(move || {
                let mut second = Store::open(&path, &team()).unwrap();
                second.write(|tx| add_note(tx, "other"))
            });
            // Long enough for a writer that is not held back to commit.
            thread::sleep(Duration::from_millis(400));
            add_note(tx, "first")?;
            Ok(other)
        })
        .expect("the first write was overtaken by another writer");
    assert_eq!(other.join().unwrap(), Ok(()));
    assert_eq!(count_notes(&first), 2);
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

mod two_writers {
    use std::thread;

    use serde_json::json;
    use teamwire_core::{
        Actor, EventType, HarnessId, HostId, LeadRoute, MemberKind, MessageType, Mode, Recipient,
        Role, TeamName, Timestamp,
    };
    use teamwire_fakes::TestHome;
    use teamwire_store::{NewMember, NewMessage, NewTeam, Store};

    const ROUNDS: u32 = 60;

    fn at(n: u32) -> Timestamp {
        Timestamp::from_unix_millis(1_790_000_000_000 + i64::from(n)).unwrap()
    }

    fn member(role: &Role) -> NewMember {
        NewMember {
            role: role.clone(),
            title: "t".to_owned(),
            kind: MemberKind::Implementer,
            mode: Mode::Pane,
            harness: HarnessId::from_static("claude-code"),
            host: HostId::from_static("herdr"),
            cwd: "/w".into(),
            repo: None,
            branch: None,
            base: None,
            base_sha: None,
            check: None,
            owns: vec![],
            deadline_s: 60,
        }
    }

    #[test]
    fn two_writers_two_members_no_loss() {
        let home = TestHome::new().unwrap();
        let team = TeamName::parse("alpha").unwrap();
        let roles = [Role::parse("api").unwrap(), Role::parse("web").unwrap()];
        let mut setup = Store::open(home.path(), &team).unwrap();
        setup
            .write(|tx| {
                tx.create_team(
                    at(0),
                    &NewTeam {
                        name: team.clone(),
                        title: "t".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: None,
                    },
                )?;
                for r in &roles {
                    tx.insert_member(at(0), &member(r))?;
                }
                Ok(())
            })
            .unwrap();

        // Each writer has its own connection and changes only its own member:
        // its gate marker, its messages and its events.
        let handles: Vec<_> = roles
            .clone()
            .into_iter()
            .map(|role| {
                let path = home.path().to_path_buf();
                let team = team.clone();
                thread::spawn(move || {
                    let mut store = Store::open_existing(&path, &team).unwrap();
                    for i in 1..=ROUNDS {
                        store
                            .write(|tx| {
                                tx.set_gate_told_generation(&role, i)?;
                                tx.insert_message(
                                    at(i),
                                    &NewMessage {
                                        sender: Actor::Member(role.clone()),
                                        kind: MessageType::Note,
                                        re: None,
                                        commit: None,
                                        body: format!("{role} {i}"),
                                        client_key: None,
                                        recipients: vec![Recipient::Lead],
                                    },
                                )?;
                                tx.record_event(
                                    at(i),
                                    EventType::WatchLine,
                                    Some(&role),
                                    None,
                                    &json!({ "i": i }),
                                )?;
                                Ok(())
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let (members, messages, lines) = setup
            .read(|tx| {
                let members = tx.members()?.all_or_corrupt()?;
                let messages = tx
                    .messages(&teamwire_store::MessageFilter::default())?
                    .all_or_corrupt()?;
                let lines = tx.events_of_type(EventType::WatchLine)?.all_or_corrupt()?;
                Ok((members, messages, lines))
            })
            .unwrap();
        for m in &members {
            assert_eq!(
                m.gate_told_generation,
                Some(ROUNDS),
                "{} lost an update",
                m.role
            );
        }
        assert_eq!(messages.len(), 2 * ROUNDS as usize);
        for role in &roles {
            let mut seqs: Vec<u64> = messages
                .iter()
                .filter(|m| m.sender == Actor::Member(role.clone()))
                .map(|m| m.seq)
                .collect();
            seqs.sort_unstable();
            assert_eq!(seqs, (1..=u64::from(ROUNDS)).collect::<Vec<_>>(), "{role}");
        }
        let mut ids: Vec<&str> = messages.iter().map(|m| m.display_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), messages.len(), "a display id was used twice");
        assert_eq!(lines.len(), 2 * ROUNDS as usize);
        assert_eq!(setup.integrity_check().unwrap(), "ok");
    }
}
