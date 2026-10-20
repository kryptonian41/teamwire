#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use teamwire_core::EventType;

use crate::testkit::{at, count_events, last_event, role, with_members};
use crate::{NewVerifyRecord, Waiver};

fn record() -> NewVerifyRecord {
    NewVerifyRecord {
        role: role("api"),
        head_sha: "abc1234".to_owned(),
        verdict: "flagged".to_owned(),
        exit_code: Some(0),
        seconds: Some(12),
        flags: vec!["ignore_added".to_owned()],
        log_sha256: Some([3; 32]),
    }
}

fn waiver(flag: &str, head: &str) -> Waiver {
    Waiver {
        role: role("api"),
        flag_id: flag.to_owned(),
        head_sha: head.to_owned(),
        reason: "fixture".to_owned(),
        created_at: at(0),
    }
}

#[test]
fn a_verify_record_is_stored_with_its_event() {
    let mut f = with_members(&["api"]);
    let id = f
        .store
        .write(|tx| tx.insert_verify_record(at(50), &record()))
        .unwrap();
    let rows = f
        .store
        .read(|tx| tx.verify_records(&role("api")))
        .unwrap()
        .items;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].id, rows[0].recorded_at), (id, at(50)));
    assert_eq!(rows[0].record, record());
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::VerifyRecorded);
    assert_eq!(e.role, Some(role("api")));
    assert_eq!(e.data["verdict"], "flagged");
    assert_eq!(e.data["flags"], serde_json::json!(["ignore_added"]));
    assert_eq!(e.data["exit"], 0);
    assert_eq!(e.data["seconds"], 12);
    assert_eq!(e.data["head_sha"], record().head_sha);
}

#[test]
fn a_record_for_an_unknown_member_is_refused() {
    let mut f = with_members(&["web"]);
    assert!(
        f.store
            .write(|tx| tx.insert_verify_record(at(1), &record()))
            .is_err()
    );
    assert_eq!(count_events(&f.store, EventType::VerifyRecorded), 0);
}

#[test]
fn a_waiver_binds_to_a_head_and_is_written_once() {
    let mut f = with_members(&["api"]);
    assert!(
        f.store
            .write(|tx| tx.insert_waiver(at(60), &waiver("ignore_added", "abc1234")))
            .unwrap()
    );
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::VerifyWaived);
    assert_eq!(e.data["flags"], serde_json::json!(["ignore_added"]));
    assert!(
        !f.store
            .write(|tx| tx.insert_waiver(at(61), &waiver("ignore_added", "abc1234")))
            .unwrap()
    );
    assert_eq!(count_events(&f.store, EventType::VerifyWaived), 1);
    // Another head, another waiver.
    assert!(
        f.store
            .write(|tx| tx.insert_waiver(at(62), &waiver("ignore_added", "def5678")))
            .unwrap()
    );
    let here = f
        .store
        .read(|tx| tx.waivers(&role("api"), "abc1234"))
        .unwrap()
        .items;
    assert_eq!(here.len(), 1);
    assert_eq!(
        here[0].created_at,
        at(60),
        "the waiver kept the caller's time, not the input's"
    );
    assert!(
        f.store
            .read(|tx| tx.waivers(&role("api"), "zzz"))
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn all_waivers_are_listed_across_roles_and_heads() {
    let mut f = with_members(&["api", "web"]);
    let mut other = waiver("a", "h1");
    other.role = role("web");
    for w in [waiver("a", "h1"), waiver("b", "h2"), other] {
        f.store.write(|tx| tx.insert_waiver(at(1), &w)).unwrap();
    }
    let all = f.store.read(|tx| tx.all_waivers()).unwrap().items;
    assert_eq!(all.len(), 3);
    assert_eq!(all[2].role, role("web"));
}

#[test]
fn message_bodies_return_a_body_that_fails_its_hash() {
    use teamwire_core::{Actor, MessageType, Recipient};
    let mut f = with_members(&["api", "web"]);
    for body in ["first", "second"] {
        f.store
            .write(|tx| {
                tx.insert_message(
                    at(10),
                    &crate::NewMessage {
                        sender: Actor::Member(role("api")),
                        kind: MessageType::Finding,
                        re: None,
                        commit: None,
                        body: body.to_owned(),
                        client_key: None,
                        recipients: vec![Recipient::Member(role("web"))],
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    }
    f.store
        .write(|tx| {
            tx.connection()
                .execute(
                    "UPDATE messages SET body = 'edited' WHERE display_id = 'm002'",
                    [],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    let rows = f.store.read(|tx| tx.message_bodies()).unwrap();
    let got: Vec<_> = rows
        .iter()
        .map(|(id, body, sha)| (id.as_str(), body.as_str(), sha.len()))
        .collect();
    assert_eq!(got, [("m001", "first", 32), ("m002", "edited", 32)]);
}
