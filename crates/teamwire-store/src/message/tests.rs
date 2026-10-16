#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use teamwire_core::{Actor, DeliveryState, EventType, MessageType, Recipient};

use crate::testkit::{at, count_events, last_event, member, role, with_members};
use crate::{MessageFilter, NewMessage, StoreError};

fn note(sender: Actor, to: &[Recipient], body: &str, key: Option<&str>) -> NewMessage {
    NewMessage {
        sender,
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: body.to_owned(),
        client_key: key.map(str::to_owned),
        recipients: to.to_vec(),
    }
}

fn from(name: &str) -> Actor {
    Actor::Member(role(name))
}

#[test]
fn insert_writes_message_pending_deliveries_and_one_event() {
    let mut f = with_members(&["api", "web"]);
    let stored = f
        .store
        .write(|tx| {
            tx.insert_message(
                at(10),
                &note(
                    from("api"),
                    &[member("web"), Recipient::Lead],
                    "hello",
                    None,
                ),
            )
        })
        .unwrap();
    assert!(!stored.duplicate);
    let m = &stored.message;
    assert_eq!((m.display_id.as_str(), m.seq), ("m001", 1));
    assert_eq!(m.created_at, at(10));
    assert_eq!(m.body_sha256, crate::tx::sha256(b"hello"));
    assert_eq!(stored.deliveries.len(), 2);
    assert!(
        stored
            .deliveries
            .iter()
            .all(|d| d.delivery.state == DeliveryState::Pending)
    );
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::MessageSent);
    assert_eq!(event.subject, Some(m.ulid));
    assert_eq!(event.role, Some(role("api")));
    assert_eq!(event.data["recipients"], serde_json::json!(["web", "lead"]));
    assert_eq!(event.data["chars"], 5);
    assert_eq!(event.data["sender_kind"], "member");
}

#[test]
fn display_ids_are_global_and_seq_is_per_sender() {
    let mut f = with_members(&["api", "web"]);
    let mut got = Vec::new();
    for (sender, body) in [("api", "a1"), ("web", "w1"), ("api", "a2"), ("web", "w2")] {
        let s = f
            .store
            .write(|tx| {
                tx.insert_message(at(10), &note(from(sender), &[Recipient::Lead], body, None))
            })
            .unwrap();
        got.push((s.message.display_id.to_string(), s.message.seq));
    }
    assert_eq!(
        got,
        [
            ("m001".into(), 1),
            ("m002".into(), 1),
            ("m003".into(), 2),
            ("m004".into(), 2)
        ]
    );
    let lead = f
        .store
        .write(|tx| tx.insert_message(at(11), &note(Actor::Lead, &[member("api")], "l1", None)))
        .unwrap();
    assert_eq!(
        (lead.message.seq, lead.message.sender.clone()),
        (1, Actor::Lead)
    );
    assert_eq!(last_event(&f.store).role, None);
    assert_eq!(last_event(&f.store).data["sender_kind"], "lead");
}

#[test]
fn the_same_client_key_returns_the_first_message_and_writes_nothing() {
    let mut f = with_members(&["api"]);
    let new = note(from("api"), &[Recipient::Lead], "once", Some("k1"));
    let first = f.store.write(|tx| tx.insert_message(at(10), &new)).unwrap();
    let events = count_events(&f.store, EventType::MessageSent);
    let again = f.store.write(|tx| tx.insert_message(at(20), &new)).unwrap();
    assert!(again.duplicate);
    assert_eq!(again.message, first.message);
    assert_eq!(again.deliveries.len(), 1);
    assert_eq!(count_events(&f.store, EventType::MessageSent), events);
    // The counters did not move: the next message is m002 with seq 2.
    let next = f
        .store
        .write(|tx| {
            tx.insert_message(
                at(21),
                &note(from("api"), &[Recipient::Lead], "two", Some("k2")),
            )
        })
        .unwrap();
    assert_eq!(
        (next.message.display_id.as_str(), next.message.seq),
        ("m002", 2)
    );
}

#[test]
fn without_a_key_the_ulid_is_the_key() {
    let mut f = with_members(&["api"]);
    let s = f
        .store
        .write(|tx| tx.insert_message(at(1), &note(from("api"), &[Recipient::Lead], "x", None)))
        .unwrap();
    let key: String = f
        .store
        .read(|tx| {
            Ok(tx
                .connection()
                .query_row("SELECT client_key FROM messages", [], |r| r.get(0))?)
        })
        .unwrap();
    assert_eq!(key, s.message.ulid.to_string());
    let found = f
        .store
        .read(|tx| tx.message_by_client_key(&from("api"), &key))
        .unwrap();
    assert_eq!(found.map(|m| m.ulid), Some(s.message.ulid));
}

#[test]
fn a_message_needs_a_recipient_and_a_failed_insert_leaves_no_trace() {
    let mut f = with_members(&["api"]);
    let none = f
        .store
        .write(|tx| tx.insert_message(at(1), &note(from("api"), &[], "x", None)));
    assert!(matches!(none, Err(StoreError::Constraint(_))));
    // A duplicate recipient is stored once.
    let s = f
        .store
        .write(|tx| {
            tx.insert_message(
                at(2),
                &note(from("api"), &[Recipient::Lead, Recipient::Lead], "x", None),
            )
        })
        .unwrap();
    assert_eq!(s.deliveries.len(), 1);
    // The refused insert did not spend a display id.
    assert_eq!(s.message.display_id.as_str(), "m001");
}

#[test]
fn lookups_by_ulid_display_id_and_filter() {
    let mut f = with_members(&["api", "web"]);
    let mut ids = Vec::new();
    for (sender, to, body) in [
        ("api", "web", "one"),
        ("web", "api", "two"),
        ("api", "web", "three"),
    ] {
        let s = f
            .store
            .write(|tx| tx.insert_message(at(5), &note(from(sender), &[member(to)], body, None)))
            .unwrap();
        ids.push(s.message);
    }
    let by_display = f
        .store
        .read(|tx| tx.message_by_display_id(&ids[1].display_id))
        .unwrap();
    assert_eq!(by_display.as_ref(), Some(&ids[1]));
    assert_eq!(
        f.store
            .read(|tx| tx.message(&ids[2].ulid))
            .unwrap()
            .as_ref(),
        Some(&ids[2])
    );
    let filter = MessageFilter {
        sender: Some(from("api")),
        recipient: Some(member("web")),
        ..MessageFilter::default()
    };
    let got = f.store.read(|tx| tx.messages(&filter)).unwrap().items;
    assert_eq!(
        got.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
        ["one", "three"]
    );
    let limited = MessageFilter {
        limit: Some(1),
        ..MessageFilter::default()
    };
    assert_eq!(
        f.store
            .read(|tx| tx.messages(&limited))
            .unwrap()
            .items
            .len(),
        1
    );
    let none = MessageFilter {
        since: Some(at(6)),
        ..MessageFilter::default()
    };
    assert!(
        f.store
            .read(|tx| tx.messages(&none))
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn sent_to_since_lists_recent_messages_with_their_delivery_state() {
    let mut f = with_members(&["api", "web"]);
    for (t, body) in [(10, "early"), (100, "late")] {
        f.store
            .write(|tx| tx.insert_message(at(t), &note(from("api"), &[member("web")], body, None)))
            .unwrap();
    }
    let sent = f
        .store
        .read(|tx| tx.sent_to_since(&from("api"), &member("web"), at(50)))
        .unwrap()
        .items;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].message.body, "late");
    assert_eq!(sent[0].state, DeliveryState::Pending);
}

#[test]
fn counters_are_readable() {
    let mut f = with_members(&["api"]);
    assert_eq!(f.store.read(|tx| tx.counter("display_id")).unwrap(), None);
    f.store
        .write(|tx| tx.insert_message(at(1), &note(from("api"), &[Recipient::Lead], "x", None)))
        .unwrap();
    assert_eq!(
        f.store.read(|tx| tx.counter("display_id")).unwrap(),
        Some(1)
    );
    assert_eq!(f.store.read(|tx| tx.counter("seq:api")).unwrap(), Some(1));
}

#[test]
fn inbox_returns_a_tampered_body_with_hash_ok_false() {
    let mut f = with_members(&["api", "web"]);
    for body in ["one", "two"] {
        f.store
            .write(|tx| tx.insert_message(at(5), &note(from("api"), &[member("web")], body, None)))
            .unwrap();
    }
    f.store
        .write(|tx| {
            tx.connection().execute(
                "UPDATE messages SET body = 'edited' WHERE display_id = 'm001'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let states = [DeliveryState::Pending, DeliveryState::HandedOff];
    let inbox = f
        .store
        .read(|tx| tx.inbox(&member("web"), &states))
        .unwrap();
    assert!(inbox.bad.is_empty());
    let got: Vec<(&str, bool)> = inbox
        .items
        .iter()
        .map(|e| (e.message.body.as_str(), e.hash_ok))
        .collect();
    assert_eq!(got, [("edited", false), ("two", true)]);
    assert_eq!(
        inbox.items[0].delivery.delivery.state,
        DeliveryState::Pending
    );
    // The other reads still refuse the damaged message.
    let all = f
        .store
        .read(|tx| tx.messages(&MessageFilter::default()))
        .unwrap();
    assert_eq!((all.items.len(), all.bad.len()), (1, 1));
}

#[test]
fn messages_to_or_from_a_role_and_the_newest_n() {
    let mut f = with_members(&["api", "web", "doc"]);
    for (s, to, body) in [
        ("api", "web", "a>w"),
        ("web", "doc", "w>d"),
        ("doc", "api", "d>a"),
        ("api", "doc", "a>d"),
    ] {
        f.store
            .write(|tx| tx.insert_message(at(5), &note(from(s), &[member(to)], body, None)))
            .unwrap();
    }
    let bodies = |f: &crate::testkit::Fixture, filter: MessageFilter| -> Vec<String> {
        f.store
            .read(|tx| tx.messages(&filter))
            .unwrap()
            .items
            .into_iter()
            .map(|m| m.body)
            .collect()
    };
    let web = MessageFilter {
        involving: Some(from("web")),
        ..MessageFilter::default()
    };
    assert_eq!(bodies(&f, web), ["a>w", "w>d"]);
    let last_two = MessageFilter {
        limit: Some(2),
        newest: true,
        ..MessageFilter::default()
    };
    assert_eq!(bodies(&f, last_two), ["d>a", "a>d"]);
    let oldest_two = MessageFilter {
        limit: Some(2),
        ..MessageFilter::default()
    };
    assert_eq!(bodies(&f, oldest_two), ["a>w", "w>d"]);
    let api_last = MessageFilter {
        involving: Some(from("api")),
        limit: Some(1),
        newest: true,
        ..MessageFilter::default()
    };
    assert_eq!(bodies(&f, api_last), ["a>d"]);
}

#[test]
fn a_question_stays_unanswered_until_an_answer_points_at_it() {
    let mut f = with_members(&["api", "web"]);
    let ask = |kind, re, body: &str| NewMessage {
        kind,
        re,
        ..note(from("api"), &[member("web")], body, None)
    };
    let first = f
        .store
        .write(|tx| {
            let a = tx.insert_message(at(1), &ask(MessageType::Question, None, "one?"))?;
            tx.insert_message(at(2), &ask(MessageType::Question, None, "two?"))?;
            tx.insert_message(at(3), &ask(MessageType::Finding, None, "not a question"))?;
            Ok(a.message.ulid)
        })
        .unwrap();
    let open = |f: &crate::testkit::Fixture| {
        f.store
            .read(|tx| tx.unanswered_questions())
            .unwrap()
            .items
            .into_iter()
            .map(|m| m.body)
            .collect::<Vec<_>>()
    };
    assert_eq!(open(&f), ["one?", "two?"]);
    f.store
        .write(|tx| tx.insert_message(at(4), &ask(MessageType::Answer, Some(first), "yes")))
        .unwrap();
    assert_eq!(open(&f), ["two?"]);
}
