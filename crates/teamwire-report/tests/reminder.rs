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

use teamwire_core::{EventType, TurnEndReply};
use teamwire_report::{on_tool_use, on_turn_end};

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
    assert!(
        note.contains("Read them with the team_inbox tool"),
        "{note}"
    );
    // A member in the middle of its turn is not told to stop.
    assert!(!note.contains("otherwise stop"), "{note}");
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

#[test]
fn a_message_the_typed_line_carried_whole_is_not_reminded() {
    let (mut rig, api) = team();
    rig.message_from_lead(
        &api,
        "rebase on main",
        teamwire_core::DeliveryState::HandedOff,
    );
    assert_eq!(
        on_tool_use(&mut rig.ctx(), &api),
        None,
        "the line held it all"
    );
    assert!(rig.events(EventType::DeliveryReminded).is_empty());
}

#[test]
fn a_short_message_the_host_never_took_is_still_reminded() {
    let (mut rig, api) = team();
    let id = rig.message_from_lead(
        &api,
        "rebase on main",
        teamwire_core::DeliveryState::Undelivered,
    );
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&id), "{note}");
}

/// A message from the lead to `who`, held for a handoff (ADR 0043 decision
/// 11): `undelivered` with `held_for_handoff`, never typed.
fn held_from_lead(rig: &mut rig::Rig, who: &teamwire_core::Role) -> String {
    let held = rig.message_from_lead(
        who,
        "one more thing",
        teamwire_core::DeliveryState::Undelivered,
    );
    let recipient = teamwire_core::Recipient::Member(who.clone());
    let at = kit::at(3);
    rig.store
        .write(|tx| {
            let message = tx
                .reader()
                .message_by_display_id(&teamwire_core::DisplayId::parse(&held).unwrap())?
                .unwrap();
            tx.set_delivery_state(
                at,
                &message.ulid,
                &recipient,
                &teamwire_store::DeliveryUpdate {
                    state: teamwire_core::DeliveryState::Undelivered,
                    route: None,
                    error: Some(teamwire_core::DeliveryError::new(
                        teamwire_core::DeliveryErrorCode::HeldForHandoff,
                        "the member's newest result asked for a handoff",
                    )),
                    attempts: 0,
                },
            )
        })
        .unwrap();
    held
}

/// A held message waits for the fresh session: the old session's in-turn
/// reminder never names it, so that session does no more work for it. The
/// message beside it, left `undelivered` with no hold, is still named.
#[test]
fn a_message_held_for_a_handoff_is_not_reminded() {
    let (mut rig, api) = team();
    let held = held_from_lead(&mut rig, &api);
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None);
    let other = rig.message_from_lead(
        &api,
        "rebase on main",
        teamwire_core::DeliveryState::Undelivered,
    );
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&other) && !note.contains(&held), "{note}");
}

/// The old session's Stop hook does not hold its stop for a held message.
#[test]
fn a_message_held_for_a_handoff_does_not_hold_the_stop() {
    let (mut rig, api) = team();
    held_from_lead(&mut rig, &api);
    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &kit::stop("api-1")),
        TurnEndReply::Continue
    );
    assert!(rig.events(EventType::MemberHeld).is_empty());
}
