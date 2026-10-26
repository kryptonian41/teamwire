//! ADR 0028: a turn that ends while a sender still holds a delivery to the
//! member `pending` leaves a `delivery.recipient_stopped` mark, so the
//! sender knows no later stop will name the message.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a failing unwrap or index in an integration test fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/kit.rs"]
mod kit;
#[path = "../../teamwire-deliver/tests/support/members.rs"]
mod members;
#[path = "../../teamwire-deliver/tests/support/rig.rs"]
mod rig;

use std::time::Duration;

use teamwire_core::{
    Actor, Clock as _, EventType, MessageType, Recipient, Role, TurnEnd, TurnEndReply, Ulid,
};
use teamwire_report::on_turn_end;
use teamwire_store::NewMessage;

use kit::{stop, team};
use rig::Rig;

/// A message from the lead to `who` whose sender has not recorded an
/// outcome yet: its row is `pending`, as during a send.
fn pending_to(rig: &mut Rig, who: &Role) -> (Ulid, String) {
    let at = rig.clock.now();
    let body = format!("being sent {}", "and more ".repeat(40));
    rig.store
        .write(|tx| {
            let stored = tx.insert_message(
                at,
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body,
                    client_key: None,
                    recipients: vec![Recipient::Member(who.clone())],
                },
            )?;
            Ok((stored.message.ulid, stored.message.display_id.to_string()))
        })
        .unwrap()
}

fn stops_of(rig: &Rig, message: &Ulid) -> Vec<teamwire_core::Event> {
    rig.events(EventType::DeliveryRecipientStopped)
        .into_iter()
        .filter(|e| e.subject.as_ref() == Some(message))
        .collect()
}

#[test]
fn a_stop_while_a_delivery_is_pending_leaves_the_mark_and_no_hold() {
    let (mut rig, api) = team();
    let (message, display) = pending_to(&mut rig, &api);

    let reply = on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));

    assert!(
        !matches!(&reply, TurnEndReply::Hold(note) if note.contains(&display)),
        "its sender may still type it: {reply:?}"
    );
    let stops = stops_of(&rig, &message);
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0].role.as_ref(), Some(&api));
    assert_eq!(stops[0].data["to"], "api");
}

#[test]
fn the_stop_after_a_hold_leaves_the_mark_too() {
    let (mut rig, api) = team();
    let (message, _) = pending_to(&mut rig, &api);
    let after_hold = TurnEnd {
        hold_already_active: true,
        ..stop("api-1")
    };

    on_turn_end(&mut rig.ctx(), &api, &after_hold);

    assert_eq!(stops_of(&rig, &message).len(), 1);
}

#[test]
fn a_pending_delivery_left_by_a_crash_is_named_and_not_marked() {
    let (mut rig, api) = team();
    let (message, display) = pending_to(&mut rig, &api);
    rig.clock.advance(Duration::from_secs(61));

    let reply = on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));

    assert!(
        matches!(&reply, TurnEndReply::Hold(note) if note.contains(&display)),
        "{reply:?}"
    );
    assert!(stops_of(&rig, &message).is_empty());
}

#[test]
fn a_subagent_stop_leaves_no_mark() {
    let (mut rig, api) = team();
    let (message, _) = pending_to(&mut rig, &api);
    let subagent = TurnEnd {
        subagent: true,
        ..stop("api-1")
    };

    on_turn_end(&mut rig.ctx(), &api, &subagent);

    assert!(stops_of(&rig, &message).is_empty());
}
