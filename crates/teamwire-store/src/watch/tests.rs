#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]

use serde_json::json;
use teamwire_core::{EventType, Pid};

use crate::LeaseOutcome;
use crate::testkit::{at, count_events, fixture, last_event};

#[test]
fn the_first_watch_takes_the_lease_and_a_live_second_is_refused() {
    let mut f = fixture();
    let out = f
        .store
        .write(|tx| tx.take_watch_lease(at(1), Pid::new(10), "mac", |_| false))
        .unwrap();
    assert_eq!(out, LeaseOutcome::Taken);
    assert_eq!(last_event(&f.store).kind, EventType::WatchStarted);
    let second = f
        .store
        .write(|tx| tx.take_watch_lease(at(2), Pid::new(20), "mac", |_| false))
        .unwrap();
    assert!(
        matches!(&second, LeaseOutcome::HeldBy(held)
            if held.pid == Pid::new(10) && held.started_at == at(1)),
        "the lease was taken from a live watch: {second:?}"
    );
    assert_eq!(count_events(&f.store, EventType::WatchStarted), 1);
}

#[test]
fn a_dead_holders_lease_is_taken_over_and_the_same_pid_may_retake() {
    let mut f = fixture();
    f.store
        .write(|tx| tx.take_watch_lease(at(1), Pid::new(10), "mac", |_| false))
        .unwrap();
    let out = f
        .store
        .write(|tx| {
            tx.take_watch_lease(at(9), Pid::new(20), "mac", |held| held.pid == Pid::new(10))
        })
        .unwrap();
    assert_eq!(out, LeaseOutcome::Taken);
    let lease = f.store.read(|tx| tx.watch_lease()).unwrap().unwrap();
    assert_eq!((lease.pid, lease.started_at), (Pid::new(20), at(9)));
    let same = f
        .store
        .write(|tx| tx.take_watch_lease(at(10), Pid::new(20), "mac", |_| false))
        .unwrap();
    assert_eq!(same, LeaseOutcome::Taken);
}

#[test]
fn heartbeat_moves_only_for_the_holder() {
    let mut f = fixture();
    f.store
        .write(|tx| tx.take_watch_lease(at(1), Pid::new(10), "mac", |_| false))
        .unwrap();
    assert!(
        f.store
            .write(|tx| tx.heartbeat_watch_lease(at(30), Pid::new(10)))
            .unwrap()
    );
    assert_eq!(
        f.store
            .read(|tx| tx.watch_lease())
            .unwrap()
            .unwrap()
            .heartbeat_at,
        at(30)
    );
    assert!(
        !f.store
            .write(|tx| tx.heartbeat_watch_lease(at(40), Pid::new(99)))
            .unwrap()
    );
    assert_eq!(
        f.store
            .read(|tx| tx.watch_lease())
            .unwrap()
            .unwrap()
            .heartbeat_at,
        at(30)
    );
}

#[test]
fn release_frees_the_lease_with_an_event_and_only_for_the_holder() {
    let mut f = fixture();
    f.store
        .write(|tx| tx.take_watch_lease(at(1), Pid::new(10), "mac", |_| false))
        .unwrap();
    assert!(
        !f.store
            .write(|tx| tx.release_watch_lease(at(2), Pid::new(99), &json!({})))
            .unwrap()
    );
    assert_eq!(count_events(&f.store, EventType::WatchStopped), 0);
    assert!(
        f.store
            .write(|tx| tx.release_watch_lease(
                at(3),
                Pid::new(10),
                &json!({ "events": ["TEAM-COMPLETE"] })
            ))
            .unwrap()
    );
    assert_eq!(f.store.read(|tx| tx.watch_lease()).unwrap(), None);
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::WatchStopped);
    assert_eq!(e.data["events"], json!(["TEAM-COMPLETE"]));
}

#[test]
fn announce_once_says_yes_exactly_once_per_key() {
    let mut f = fixture();
    assert!(
        f.store
            .write(|tx| tx.announce_once(at(1), "LATE:api:2"))
            .unwrap()
    );
    assert!(
        !f.store
            .write(|tx| tx.announce_once(at(2), "LATE:api:2"))
            .unwrap()
    );
    assert!(
        f.store
            .write(|tx| tx.announce_once(at(3), "LATE:api:3"))
            .unwrap()
    );
    assert!(f.store.read(|tx| tx.was_announced("LATE:api:2")).unwrap());
    assert!(!f.store.read(|tx| tx.was_announced("DIED:api:1")).unwrap());
}

#[test]
fn announcements_are_listed_all_and_by_key_prefix() {
    let mut f = fixture();
    for (t, key) in [
        (1, "SILENT:api:1:1"),
        (2, "LATE:api:1"),
        (3, "SILENT:api:2:1"),
        (4, "SILENT:web:1:1"),
    ] {
        f.store.write(|tx| tx.announce_once(at(t), key)).unwrap();
    }
    let all = f.store.read(|tx| tx.announcements()).unwrap().items;
    assert_eq!(all.len(), 4);
    assert_eq!((all[0].key.as_str(), all[0].at), ("SILENT:api:1:1", at(1)));
    let silent_api = f
        .store
        .read(|tx| tx.announcements_with_prefix("SILENT:api:"))
        .unwrap()
        .items;
    let keys: Vec<&str> = silent_api.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, ["SILENT:api:1:1", "SILENT:api:2:1"]);
    // A prefix with SQL wildcard characters matches literally.
    assert!(
        f.store
            .read(|tx| tx.announcements_with_prefix("%"))
            .unwrap()
            .items
            .is_empty()
    );
}
