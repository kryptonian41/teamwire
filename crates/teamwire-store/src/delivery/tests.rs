#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use teamwire_core::{
    Actor, DeliveryError, DeliveryErrorCode, DeliveryState, EventType, MessageType, Recipient,
    Route, Ulid,
};

use crate::testkit::{at, count_events, last_event, member, role, with_members};
use crate::{DeliveryUpdate, NewMessage, StoreError};

fn send_one(f: &mut crate::testkit::Fixture) -> Ulid {
    f.store
        .write(|tx| {
            tx.insert_message(
                at(100),
                &NewMessage {
                    sender: Actor::Member(role("api")),
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: "hi".to_owned(),
                    client_key: None,
                    recipients: vec![member("web")],
                },
            )
        })
        .unwrap()
        .message
        .ulid
}

fn update(state: DeliveryState, attempts: u32) -> DeliveryUpdate {
    DeliveryUpdate {
        state,
        route: Some(Route::HostTyping),
        error: None,
        attempts,
    }
}

fn state(f: &crate::testkit::Fixture, m: &Ulid) -> DeliveryState {
    f.store
        .read(|tx| tx.delivery(m, &member("web")))
        .unwrap()
        .unwrap()
        .delivery
        .state
}

#[test]
fn handed_off_records_route_attempts_and_seconds() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    f.store
        .write(|tx| {
            tx.set_delivery_state(
                at(102),
                &m,
                &member("web"),
                &update(DeliveryState::HandedOff, 1),
            )
        })
        .unwrap();
    let d = f
        .store
        .read(|tx| tx.delivery(&m, &member("web")))
        .unwrap()
        .unwrap();
    assert_eq!(
        (d.delivery.state, d.delivery.attempts),
        (DeliveryState::HandedOff, 1)
    );
    assert_eq!(d.delivery.route, Some(Route::HostTyping));
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::DeliveryHandedOff);
    assert_eq!(e.subject, Some(m));
    assert_eq!(e.role, Some(role("web")));
    assert_eq!(e.data["seconds"], 2.0);
    assert_eq!(e.data["attempts"], 1);
    assert_eq!(e.data["route"], "host_typing");
    assert_eq!(e.data["to"], "web");
}

#[test]
fn undelivered_and_unconfirmed_keep_the_error_and_write_their_events() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let err = DeliveryError::new(DeliveryErrorCode::HostNotFound, "pane gone");
    f.store
        .write(|tx| {
            tx.set_delivery_state(
                at(101),
                &m,
                &member("web"),
                &DeliveryUpdate {
                    state: DeliveryState::Undelivered,
                    route: None,
                    error: Some(err.clone()),
                    attempts: 3,
                },
            )
        })
        .unwrap();
    let d = f
        .store
        .read(|tx| tx.delivery(&m, &member("web")))
        .unwrap()
        .unwrap();
    assert_eq!(d.delivery.error, Some(err));
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::DeliveryUndelivered);
    assert_eq!(e.data["code"], "host_not_found");
    // A resend may end unconfirmed, then any later outcome.
    f.store
        .write(|tx| {
            tx.set_delivery_state(
                at(102),
                &m,
                &member("web"),
                &update(DeliveryState::Unconfirmed, 4),
            )
        })
        .unwrap();
    assert_eq!(last_event(&f.store).kind, EventType::DeliveryUnconfirmed);
    let d = f
        .store
        .read(|tx| tx.delivery(&m, &member("web")))
        .unwrap()
        .unwrap();
    assert_eq!(
        d.delivery.error, None,
        "the old error stayed after a new outcome"
    );
}

#[test]
fn states_never_move_backward() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let web = member("web");
    // pending never comes back
    let back = f
        .store
        .write(|tx| tx.set_delivery_state(at(101), &m, &web, &update(DeliveryState::Pending, 0)));
    assert!(matches!(back, Err(StoreError::Constraint(_))));
    f.store
        .write(|tx| tx.set_delivery_state(at(101), &m, &web, &update(DeliveryState::HandedOff, 1)))
        .unwrap();
    // handed_off only goes to acked
    for to in [
        DeliveryState::Pending,
        DeliveryState::Unconfirmed,
        DeliveryState::Undelivered,
        DeliveryState::HandedOff,
    ] {
        let r = f
            .store
            .write(|tx| tx.set_delivery_state(at(102), &m, &web, &update(to, 2)));
        assert!(
            matches!(r, Err(StoreError::Constraint(_))),
            "handed_off to {to}"
        );
    }
    assert_eq!(state(&f, &m), DeliveryState::HandedOff);
    assert!(
        f.store
            .write(|tx| tx.ack_delivery(at(103), &m, &web))
            .unwrap()
    );
    // acked is final, whichever method asks
    for to in DeliveryState::ALL {
        let r = f
            .store
            .write(|tx| tx.set_delivery_state(at(104), &m, &web, &update(*to, 3)));
        assert!(matches!(r, Err(StoreError::Constraint(_))), "acked to {to}");
    }
    assert!(
        f.store
            .write(|tx| tx.begin_resend(at(105), &m, &web))
            .is_err()
    );
    assert_eq!(state(&f, &m), DeliveryState::Acked);
}

#[test]
fn unconfirmed_never_becomes_undelivered() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let web = member("web");
    f.store
        .write(|tx| {
            tx.set_delivery_state(at(101), &m, &web, &update(DeliveryState::Unconfirmed, 1))
        })
        .unwrap();
    let r = f.store.write(|tx| {
        tx.set_delivery_state(at(102), &m, &web, &update(DeliveryState::Undelivered, 2))
    });
    assert!(matches!(r, Err(StoreError::Constraint(_))), "{r:?}");
    assert_eq!(state(&f, &m), DeliveryState::Unconfirmed);
}

#[test]
fn setting_acked_through_set_delivery_state_is_refused() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let r = f.store.write(|tx| {
        tx.set_delivery_state(
            at(101),
            &m,
            &member("web"),
            &update(DeliveryState::Acked, 1),
        )
    });
    assert!(matches!(r, Err(StoreError::Constraint(_))));
    assert_eq!(state(&f, &m), DeliveryState::Pending);
}

#[test]
fn ack_from_any_state_writes_one_event_and_is_idempotent() {
    for start in [
        DeliveryState::Pending,
        DeliveryState::Undelivered,
        DeliveryState::Unconfirmed,
        DeliveryState::HandedOff,
    ] {
        let mut f = with_members(&["api", "web"]);
        let m = send_one(&mut f);
        if start != DeliveryState::Pending {
            f.store
                .write(|tx| tx.set_delivery_state(at(101), &m, &member("web"), &update(start, 1)))
                .unwrap();
        }
        assert!(
            f.store
                .write(|tx| tx.ack_delivery(at(110), &m, &member("web")))
                .unwrap(),
            "{start}"
        );
        let e = last_event(&f.store);
        assert_eq!(e.kind, EventType::DeliveryAcked);
        assert_eq!(e.data["seconds"], 10.0);
        let d = f
            .store
            .read(|tx| tx.delivery(&m, &member("web")))
            .unwrap()
            .unwrap();
        assert_eq!(d.delivery.acked_at, Some(at(110)));
        assert!(
            !f.store
                .write(|tx| tx.ack_delivery(at(120), &m, &member("web")))
                .unwrap()
        );
        assert_eq!(count_events(&f.store, EventType::DeliveryAcked), 1);
        let d = f
            .store
            .read(|tx| tx.delivery(&m, &member("web")))
            .unwrap()
            .unwrap();
        assert_eq!(
            d.delivery.acked_at,
            Some(at(110)),
            "a second ack moved acked_at"
        );
    }
}

#[test]
fn a_resend_adds_an_attempt_without_changing_the_state() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let web = member("web");
    f.store
        .write(|tx| {
            tx.set_delivery_state(at(101), &m, &web, &update(DeliveryState::Undelivered, 2))
        })
        .unwrap();
    let attempt = f
        .store
        .write(|tx| tx.begin_resend(at(200), &m, &web))
        .unwrap();
    assert_eq!(attempt, 3);
    assert_eq!(state(&f, &m), DeliveryState::Undelivered);
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::DeliveryResent);
    assert_eq!(
        e.data,
        serde_json::json!({ "to": "web", "attempt": 3, "previous_state": "undelivered" })
    );
    f.store
        .write(|tx| tx.set_delivery_state(at(201), &m, &web, &update(DeliveryState::HandedOff, 3)))
        .unwrap();
    let r = f.store.write(|tx| tx.begin_resend(at(202), &m, &web));
    assert!(
        matches!(r, Err(StoreError::Constraint(_))),
        "a handed-off line was resent"
    );
}

#[test]
fn a_reminder_sets_the_time_and_writes_an_event_but_not_for_read_messages() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let web = member("web");
    f.store
        .write(|tx| tx.mark_reminded(at(130), &m, &web))
        .unwrap();
    let d = f.store.read(|tx| tx.delivery(&m, &web)).unwrap().unwrap();
    assert_eq!(d.reminded_at, Some(at(130)));
    assert_eq!(last_event(&f.store).kind, EventType::DeliveryReminded);
    f.store
        .write(|tx| tx.ack_delivery(at(140), &m, &web))
        .unwrap();
    assert!(
        f.store
            .write(|tx| tx.mark_reminded(at(150), &m, &web))
            .is_err()
    );
}

#[test]
fn a_missing_delivery_is_an_error() {
    let mut f = with_members(&["api", "web"]);
    let m = send_one(&mut f);
    let r = f
        .store
        .write(|tx| tx.ack_delivery(at(1), &m, &Recipient::Lead));
    assert!(matches!(r, Err(StoreError::Constraint(_))));
}

#[test]
fn inbox_style_reads_by_recipient_and_state() {
    let mut f = with_members(&["api", "web"]);
    let first = send_one(&mut f);
    let second = send_one(&mut f);
    let web = member("web");
    f.store
        .write(|tx| {
            tx.set_delivery_state(at(101), &first, &web, &update(DeliveryState::HandedOff, 1))
        })
        .unwrap();
    let unread = f
        .store
        .read(|tx| tx.deliveries_for(&web, &[DeliveryState::Pending, DeliveryState::HandedOff]))
        .unwrap()
        .items;
    let ids: Vec<Ulid> = unread.iter().map(|d| d.delivery.message).collect();
    assert_eq!(ids, [first, second]);
    let pending = f
        .store
        .read(|tx| tx.deliveries_in_states(&[DeliveryState::Pending]))
        .unwrap()
        .items;
    assert_eq!(pending.len(), 1);
    assert_eq!(
        f.store
            .read(|tx| tx.deliveries_of(&second))
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        f.store
            .read(|tx| tx.deliveries_for(&Recipient::Lead, &[DeliveryState::Pending]))
            .unwrap()
            .items
            .is_empty()
    );
}
