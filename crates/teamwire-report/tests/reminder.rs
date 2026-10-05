//! The in-turn reminder of unread messages.
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

use teamwire_core::EventType;
use teamwire_report::on_tool_use;

use kit::team;

#[test]
fn reminds_once_per_message_id() {
    let (mut rig, api) = team();
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None, "nothing unread");

    let first = rig.unread_from_lead(&api, "one");
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(
        note.contains(&first) && note.contains("teamwire inbox"),
        "{note}"
    );
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None, "each id once");

    let second = rig.unread_from_lead(&api, "two");
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&second) && !note.contains(&first), "{note}");
    assert_eq!(rig.events(EventType::DeliveryReminded).len(), 2);
}

#[test]
fn a_read_message_is_not_reminded() {
    let (mut rig, api) = team();
    rig.unread_from_lead(&api, "one");
    let members = rig
        .store
        .read(|tx| {
            tx.inbox(
                &teamwire_core::Recipient::Member(api.clone()),
                &[teamwire_core::DeliveryState::HandedOff],
            )
        })
        .unwrap();
    let ulid = members.items[0].message.ulid;
    let at = kit::at(5);
    rig.store
        .write(|tx| {
            tx.ack_delivery(at, &ulid, &teamwire_core::Recipient::Member(api.clone()))
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None);
}

#[test]
fn a_pending_delivery_older_than_a_minute_is_reminded_once() {
    let (mut rig, api) = team();
    // A sender that crashed after the commit left the row pending.
    let at = teamwire_core::Clock::now(&*rig.clock);
    let recipient = teamwire_core::Recipient::Member(api.clone());
    let display = rig
        .store
        .write(|tx| {
            let stored = tx.insert_message(
                at,
                &teamwire_store::NewMessage {
                    sender: teamwire_core::Actor::Lead,
                    kind: teamwire_core::MessageType::Note,
                    re: None,
                    commit: None,
                    body: "left pending".to_owned(),
                    client_key: None,
                    recipients: vec![recipient],
                },
            )?;
            Ok(stored.message.display_id.to_string())
        })
        .unwrap();
    rig.clock.advance(std::time::Duration::from_secs(30));
    assert_eq!(
        on_tool_use(&mut rig.ctx(), &api),
        None,
        "its sender may still be delivering"
    );
    rig.clock.advance(std::time::Duration::from_secs(31));
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&display), "{note}");
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None, "each id once");
}
