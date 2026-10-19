#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]

use std::time::Duration;

use serde_json::json;
use teamwire_core::{Actor, EventType, MessageType, Recipient};

use crate::testkit::{at, count_events, event_types, last_event, role, with_members};
use crate::{NewMessage, StoreError};

const STALE: Duration = Duration::from_secs(300);

fn add_events(f: &mut crate::testkit::Fixture, n: u32) {
    f.store
        .write(|tx| {
            for i in 0..n {
                tx.record_event(
                    at(100 + i64::from(i)),
                    EventType::WatchLine,
                    None,
                    None,
                    &json!({ "i": i }),
                )?;
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn record_event_stores_every_field_and_the_writer_version() {
    let mut f = with_members(&["api"]);
    let subject = f
        .store
        .write(|tx| {
            tx.insert_message(
                at(5),
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: "x".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Lead],
                },
            )
        })
        .unwrap()
        .message
        .ulid;
    let id = f
        .store
        .write(|tx| {
            tx.record_event(
                at(9),
                EventType::MessageRefused,
                Some(&role("api")),
                Some(&subject),
                &json!({ "reason": "empty" }),
            )
        })
        .unwrap();
    let e = f.store.read(|tx| tx.event(&id)).unwrap().unwrap();
    assert_eq!(e.kind, EventType::MessageRefused);
    assert_eq!(
        (e.role, e.subject, e.time),
        (Some(role("api")), Some(subject), at(9))
    );
    assert_eq!(e.data, json!({ "reason": "empty" }));
    assert_eq!(e.writer_version, crate::WRITER_VERSION);
    let about = f.store.read(|tx| tx.events_about(&subject)).unwrap().items;
    assert_eq!(
        about.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [EventType::MessageSent, EventType::MessageRefused]
    );
    assert_eq!(
        f.store
            .read(|tx| tx.events_of_type(EventType::MessageRefused))
            .unwrap()
            .items
            .len(),
        1
    );
}

#[test]
fn event_ulids_carry_the_callers_time() {
    let mut f = with_members(&[]);
    let id = f
        .store
        .write(|tx| tx.record_event(at(77), EventType::WatchLine, None, None, &json!({})))
        .unwrap();
    let text = id.to_string();
    let parsed = ulid::Ulid::from_string(&text).unwrap();
    assert_eq!(
        i64::try_from(parsed.timestamp_ms()).unwrap(),
        at(77).unix_millis()
    );
}

#[test]
fn a_failed_transaction_leaves_no_event() {
    let mut f = with_members(&["api"]);
    let before = event_types(&f.store).len();
    let r: Result<(), StoreError> = f.store.write(|tx| {
        tx.record_event(at(9), EventType::WatchLine, None, None, &json!({}))?;
        Err(StoreError::Io("boom".into()))
    });
    assert!(r.is_err());
    assert_eq!(event_types(&f.store).len(), before);
}

#[test]
fn a_store_error_is_recorded_with_op_and_code() {
    let mut f = with_members(&[]);
    f.store
        .write(|tx| tx.record_store_error(at(3), "send", &StoreError::Busy))
        .unwrap();
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::StoreError);
    assert_eq!(e.data, json!({ "op": "send", "code": "busy" }));
}

fn store_errors(store: &crate::Store) -> Vec<serde_json::Value> {
    crate::testkit::events(store)
        .into_iter()
        .filter(|e| e.kind == EventType::StoreError)
        .map(|e| e.data)
        .collect()
}

#[test]
fn a_busy_write_is_recorded_once_after_the_rollback() {
    let mut f = with_members(&[]);
    f.store
        .read(|tx| Ok(tx.connection().busy_timeout(Duration::from_millis(50))?))
        .unwrap();
    let path = crate::database_path(
        f.home.path(),
        &teamwire_core::TeamName::parse("alpha").unwrap(),
    );
    let other = rusqlite::Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let busy = f
        .store
        .write(|tx| tx.record_event(at(9), EventType::WatchLine, None, None, &json!({})));
    assert_eq!(busy.unwrap_err(), StoreError::Busy);
    other.execute_batch("ROLLBACK").unwrap();

    f.store.record_failure(at(10), "send");
    f.store.record_failure(at(11), "send");
    assert_eq!(
        store_errors(&f.store),
        [json!({ "op": "send", "code": "busy" })]
    );
    assert_eq!(last_event(&f.store).time, at(10));
}

#[test]
fn only_busy_full_corrupt_and_io_failures_are_recorded() {
    let mut f = with_members(&[]);
    for refused in [
        StoreError::SchemaNewer { db: 99, binary: 1 },
        StoreError::SchemaOlder { db: 1, binary: 6 },
        StoreError::NotTeamwire,
        StoreError::Constraint("unique".into()),
        StoreError::NoDatabase { path: "/x".into() },
    ] {
        let failed: Result<(), StoreError> = f.store.write(|_| Err(refused.clone()));
        assert!(failed.is_err());
        f.store.record_failure(at(3), "send");
    }
    assert_eq!(store_errors(&f.store), Vec::<serde_json::Value>::new());

    let failed: Result<(), StoreError> = f.store.write(|_| Err(StoreError::Corrupt("row".into())));
    assert!(failed.is_err());
    f.store.record_failure(at(4), "done");
    assert_eq!(
        store_errors(&f.store),
        [json!({ "op": "done", "code": "corrupt" })]
    );
}

#[test]
fn a_read_only_store_records_nothing() {
    let f = with_members(&[]);
    let mut ro = crate::Store::open_read_only(
        f.home.path(),
        &teamwire_core::TeamName::parse("alpha").unwrap(),
    )
    .unwrap();
    let refused =
        ro.write(|tx| tx.record_event(at(9), EventType::WatchLine, None, None, &json!({})));
    assert!(refused.is_err());
    ro.record_failure(at(10), "status");
    drop(ro);
    assert_eq!(store_errors(&f.store), Vec::<serde_json::Value>::new());
}

#[test]
fn events_after_respects_both_bounds_and_the_limit() {
    let mut f = with_members(&[]);
    let base = f.store.read(|tx| tx.last_event_seq()).unwrap();
    add_events(&mut f, 5);
    let got = f
        .store
        .read(|tx| tx.events_after(base + 1, Some(base + 4), None))
        .unwrap()
        .items;
    assert_eq!(
        got.iter().map(|e| e.seq).collect::<Vec<_>>(),
        [base + 2, base + 3, base + 4]
    );
    let limited = f
        .store
        .read(|tx| tx.events_after(base, None, Some(2)))
        .unwrap()
        .items;
    assert_eq!(limited.len(), 2);
    assert_eq!(f.store.read(|tx| tx.last_event_seq()).unwrap(), base + 5);
}

#[test]
fn the_export_claim_takes_a_batch_once_and_commit_moves_the_mark() {
    let mut f = with_members(&[]);
    assert_eq!(
        f.store.read(|tx| tx.export_mark()).unwrap().last_event_seq,
        0
    );
    add_events(&mut f, 3);
    let newest = f.store.read(|tx| tx.last_event_seq()).unwrap();
    let a = f
        .store
        .write(|tx| tx.claim_metric_export(at(200), "111", None, STALE, |_| true))
        .unwrap()
        .unwrap();
    assert_eq!((a.from_seq, a.to_seq, a.taken_over), (0, newest, false));
    // A second exporter, live or not, gets nothing while the claim is fresh.
    let b = f
        .store
        .write(|tx| tx.claim_metric_export(at(201), "222", None, STALE, |_| true))
        .unwrap();
    assert_eq!(b, None);
    // A wrong claimer or range cannot commit.
    assert!(
        !f.store
            .write(|tx| tx.commit_metric_export("222", newest))
            .unwrap()
    );
    assert!(
        !f.store
            .write(|tx| tx.commit_metric_export("111", newest - 1))
            .unwrap()
    );
    assert!(
        f.store
            .write(|tx| tx.commit_metric_export("111", newest))
            .unwrap()
    );
    let mark = f.store.read(|tx| tx.export_mark()).unwrap();
    assert_eq!(
        (mark.last_event_seq, mark.claimed_to, mark.claimed_by),
        (newest, None, None)
    );
    // Nothing new: nothing to claim.
    assert_eq!(
        f.store
            .write(|tx| tx.claim_metric_export(at(202), "111", None, STALE, |_| true))
            .unwrap(),
        None
    );
    add_events(&mut f, 1);
    let next = f
        .store
        .write(|tx| tx.claim_metric_export(at(203), "111", None, STALE, |_| true))
        .unwrap()
        .unwrap();
    assert_eq!((next.from_seq, next.to_seq), (newest, newest + 1));
}

#[test]
fn a_new_claim_stops_at_its_limit_and_a_limit_at_the_mark_claims_nothing() {
    let mut f = with_members(&[]);
    add_events(&mut f, 4);
    let newest = f.store.read(|tx| tx.last_event_seq()).unwrap();
    assert_eq!(
        f.store
            .write(|tx| tx.claim_metric_export(at(200), "111", Some(0), STALE, |_| true))
            .unwrap(),
        None,
        "nothing lies between the mark and the limit"
    );
    let a = f
        .store
        .write(|tx| tx.claim_metric_export(at(200), "111", Some(newest - 2), STALE, |_| true))
        .unwrap()
        .unwrap();
    assert_eq!((a.from_seq, a.to_seq), (0, newest - 2));
    assert!(
        f.store
            .write(|tx| tx.commit_metric_export("111", newest - 2))
            .unwrap()
    );
    // A limit past the newest event claims up to the newest event.
    let b = f
        .store
        .write(|tx| tx.claim_metric_export(at(201), "111", Some(newest + 9), STALE, |_| true))
        .unwrap()
        .unwrap();
    assert_eq!((b.from_seq, b.to_seq), (newest - 2, newest));
}

#[test]
fn a_stale_claim_of_a_dead_holder_is_taken_over_with_its_old_end() {
    let mut f = with_members(&[]);
    add_events(&mut f, 2);
    let first = f
        .store
        .write(|tx| tx.claim_metric_export(at(200), "111", None, STALE, |_| true))
        .unwrap()
        .unwrap();
    add_events(&mut f, 2); // events that arrived after the first claim
    // Old enough but the holder is alive: refused. Dead but too new: refused.
    assert_eq!(
        f.store
            .write(|tx| tx.claim_metric_export(at(600), "222", None, STALE, |_| false))
            .unwrap(),
        None
    );
    assert_eq!(
        f.store
            .write(|tx| tx.claim_metric_export(at(210), "222", None, STALE, |_| true))
            .unwrap(),
        None
    );
    let taken = f
        .store
        .write(|tx| tx.claim_metric_export(at(600), "222", None, STALE, |h| h.id == "111"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (taken.from_seq, taken.to_seq, taken.taken_over),
        (first.from_seq, first.to_seq, true)
    );
    // The dead exporter's late commit is refused; the new one's works.
    assert!(
        !f.store
            .write(|tx| tx.commit_metric_export("111", first.to_seq))
            .unwrap()
    );
    assert!(
        f.store
            .write(|tx| tx.commit_metric_export("222", taken.to_seq))
            .unwrap()
    );
}

#[test]
fn the_claim_records_its_holder_start_time_and_hands_it_to_the_check() {
    let mut f = with_members(&[]);
    add_events(&mut f, 2);
    let holder = crate::ExportHolder {
        id: "111".to_owned(),
        started: Some(at(5)),
    };
    f.store
        .write(|tx| tx.claim_metric_export(at(200), holder.clone(), None, STALE, |_| true))
        .unwrap()
        .unwrap();
    let mark = f.store.read(|tx| tx.export_mark()).unwrap();
    assert_eq!(mark.claimed_by_started, Some(at(5)));
    // The check sees the pid and the start time the claim was made with.
    let mut seen = None;
    f.store
        .write(|tx| {
            tx.claim_metric_export(at(600), "222", None, STALE, |h| {
                seen = Some(h.clone());
                false
            })
        })
        .unwrap();
    assert_eq!(seen, Some(holder));
    // Committing clears the start time with the rest of the claim.
    assert!(
        f.store
            .write(|tx| tx.commit_metric_export("111", mark.claimed_to.unwrap()))
            .unwrap()
    );
    let mark = f.store.read(|tx| tx.export_mark()).unwrap();
    assert_eq!((mark.claimed_by, mark.claimed_by_started), (None, None));
}

#[test]
fn a_failed_send_releases_the_claim_and_the_mark_stays() {
    let mut f = with_members(&[]);
    add_events(&mut f, 2);
    f.store
        .write(|tx| tx.claim_metric_export(at(200), "111", None, STALE, |_| true))
        .unwrap()
        .unwrap();
    assert!(!f.store.write(|tx| tx.release_metric_claim("222")).unwrap());
    assert!(f.store.write(|tx| tx.release_metric_claim("111")).unwrap());
    assert_eq!(
        f.store.read(|tx| tx.export_mark()).unwrap().last_event_seq,
        0
    );
    let again = f
        .store
        .write(|tx| tx.claim_metric_export(at(201), "333", None, STALE, |_| false))
        .unwrap();
    assert!(again.is_some(), "a released claim blocked the retry");
    assert_eq!(count_events(&f.store, EventType::StoreError), 0);
}

#[test]
fn events_by_time_and_by_role() {
    let mut f = with_members(&[]);
    f.store
        .write(|tx| {
            tx.record_event(
                at(100),
                EventType::MemberHeld,
                Some(&role("api")),
                None,
                &json!({}),
            )?;
            tx.record_event(
                at(200),
                EventType::MemberHeld,
                Some(&role("web")),
                None,
                &json!({}),
            )?;
            tx.record_event(at(300), EventType::WatchLine, None, None, &json!({}))?;
            Ok(())
        })
        .unwrap();
    let since = f.store.read(|tx| tx.events_since(at(200))).unwrap().items;
    assert_eq!(
        since.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [EventType::MemberHeld, EventType::WatchLine]
    );
    let api = f
        .store
        .read(|tx| tx.events_of_role(&role("api")))
        .unwrap()
        .items;
    assert!(api.iter().all(|e| e.role == Some(role("api"))));
    assert!(api.iter().any(|e| e.kind == EventType::MemberHeld));
}
