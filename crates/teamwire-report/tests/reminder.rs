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
