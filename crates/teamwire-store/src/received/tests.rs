#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use teamwire_core::{
    Actor, DeliveryState, EventType, MessageType, Pid, Recipient, Remote, TeamName, Ulid,
};

use crate::testkit::{at, count_events, last_event, member, role, with_members};
use crate::{NewMessage, ReceivedMessage, StoreError};

fn beta() -> TeamName {
    TeamName::parse("beta").unwrap()
}

fn ulid(n: u8) -> Ulid {
    Ulid::parse(&format!("01JABCDEFGHJKMNPQRSTVWXYZ{n}")).unwrap()
}

fn from_beta(id: Ulid, body: &str, to: &[Recipient]) -> ReceivedMessage {
    ReceivedMessage {
        ulid: id,
        sender: Remote::member_of(&beta(), &role("db")),
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: body.to_owned(),
        recipients: to.to_vec(),
    }
}

#[test]
fn a_received_message_keeps_its_ulid_and_takes_this_teams_ids() {
    let mut f = with_members(&["api"]);
    // The team's own messages move the display-id counter first.
    f.store
        .write(|tx| {
            tx.insert_message(
                at(5),
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: "local".to_owned(),
                    client_key: None,
                    recipients: vec![member("api")],
                },
            )
        })
        .unwrap();
    let got = f
        .store
        .write(|tx| {
            tx.insert_received(
                at(10),
                &from_beta(ulid(1), "schema moved", &[member("api"), Recipient::Lead]),
            )
        })
        .unwrap()
        .unwrap();
    assert!(!got.duplicate);
    let m = &got.message;
    assert_eq!(m.ulid, ulid(1));
    assert_eq!((m.display_id.as_str(), m.seq), ("m002", 1));
    assert_eq!(m.sender.as_str(), "db@beta");
    assert_eq!(m.created_at, at(10));
    assert_eq!(got.deliveries.len(), 2);
    assert!(
        got.deliveries
            .iter()
            .all(|d| d.delivery.state == DeliveryState::Pending)
    );
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::MessageSent);
    assert_eq!(event.role, None);
    assert_eq!(event.data["sender_kind"], "other_team");
    assert_eq!(event.data["from"], "db@beta");
}

#[test]
fn storing_the_same_message_twice_changes_nothing() {
    let mut f = with_members(&["api"]);
    let first = f
        .store
        .write(|tx| tx.insert_received(at(10), &from_beta(ulid(1), "x", &[member("api")])))
        .unwrap()
        .unwrap();
    let again = f
        .store
        .write(|tx| tx.insert_received(at(20), &from_beta(ulid(1), "x", &[member("api")])))
        .unwrap()
        .unwrap();
    assert!(again.duplicate);
    assert_eq!(again.message, first.message);
    assert_eq!(count_events(&f.store, EventType::MessageSent), 1);
}

#[test]
fn another_text_under_a_stored_ulid_is_refused() {
    let mut f = with_members(&["api"]);
    f.store
        .write(|tx| tx.insert_received(at(10), &from_beta(ulid(1), "x", &[member("api")])))
        .unwrap();
    let err = f
        .store
        .write(|tx| tx.insert_received(at(20), &from_beta(ulid(1), "y", &[member("api")])))
        .unwrap_err();
    assert!(matches!(err, StoreError::Constraint(_)), "{err}");
}

#[test]
fn a_question_this_team_never_saw_is_dropped_from_re() {
    let mut f = with_members(&["api"]);
    let mut answer = from_beta(ulid(2), "yes", &[member("api")]);
    answer.kind = MessageType::Answer;
    answer.re = Some(ulid(9));
    let got = f
        .store
        .write(|tx| tx.insert_received(at(10), &answer))
        .unwrap()
        .unwrap();
    assert_eq!(got.message.re, None);

    let question = from_beta(ulid(3), "why?", &[member("api")]);
    f.store
        .write(|tx| tx.insert_received(at(11), &question))
        .unwrap();
    answer.ulid = ulid(4);
    answer.re = Some(ulid(3));
    let got = f
        .store
        .write(|tx| tx.insert_received(at(12), &answer))
        .unwrap()
        .unwrap();
    assert_eq!(got.message.re, Some(ulid(3)));
}

#[test]
fn a_closing_team_stores_nothing() {
    let mut f = with_members(&["api"]);
    f.store
        .write(|tx| tx.begin_close(at(5), Pid::new(42)))
        .unwrap();
    let got = f
        .store
        .write(|tx| tx.insert_received(at(10), &from_beta(ulid(1), "x", &[member("api")])))
        .unwrap();
    assert!(got.is_none());
    assert_eq!(count_events(&f.store, EventType::MessageSent), 0);
}

#[test]
fn a_recipient_in_another_team_is_refused() {
    let mut f = with_members(&["api"]);
    let to = [Recipient::parse("web@gamma").unwrap()];
    let err = f
        .store
        .write(|tx| tx.insert_received(at(10), &from_beta(ulid(1), "x", &to)))
        .unwrap_err();
    assert!(matches!(err, StoreError::Constraint(_)), "{err}");
}
