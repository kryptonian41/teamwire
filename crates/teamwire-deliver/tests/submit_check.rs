//! A host that gives no acknowledgement (tmux) gets one look at the screen
//! after the submit. A line still in the input box gets one more Enter.
#![expect(
    clippy::unwrap_used,
    reason = "the shared rig unwraps its fixtures; a failure there fails the test"
)]

#[path = "support/members.rs"]
mod members;
#[path = "support/rig.rs"]
mod rig;

use std::time::Duration;

use serde_json::json;
use teamwire_core::{EventType, Liveness, Occupant, Pid, PlaceState};
use teamwire_core::{Key, SubmitAck, one_line};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeHost, PLAIN_TRUST_TEXT};
use teamwire_store::Party;

use members::role;
use rig::{Rig, sid};

fn no_ack(rig: &Rig) {
    let mut caps = FakeHost::caps();
    caps.submit_ack = SubmitAck::None;
    rig.host.set_caps(caps);
}

#[test]
fn a_line_still_in_the_box_gets_one_more_enter_and_is_not_typed_twice() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    no_ack(&rig);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello there"]);
    assert_eq!(rig.host.keys_sent(&place), [Key::Enter]);
}

#[test]
fn a_harness_that_cannot_tell_never_gets_a_second_enter() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    no_ack(&rig);
    rig.harness.set_detects_pending(false);
    deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert!(rig.host.keys_sent(&place).is_empty());
}

#[test]
fn a_line_the_session_took_gets_no_second_enter() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    no_ack(&rig);
    let host = rig.host.clone();
    let later = place.clone();
    let taken = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        host.set_screen(&later, "> hello there\n* Working (esc to interrupt)\n> ");
    });
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert!(taken.join().is_ok());
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert!(rig.host.keys_sent(&place).is_empty());
}

#[test]
fn a_dialog_on_screen_gets_no_enter() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    no_ack(&rig);
    let host = rig.host.clone();
    let later = place.clone();
    let opened = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        host.set_screen(&later, &format!("{PLAIN_TRUST_TEXT}\n> hello there"));
    });
    deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert!(opened.join().is_ok());
    assert!(rig.host.keys_sent(&place).is_empty());
}

#[test]
fn a_host_that_acknowledges_the_submit_is_not_checked() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert!(rig.host.keys_sent(&place).is_empty());
}

fn reentered(rig: &Rig) -> Vec<teamwire_core::Event> {
    rig.store
        .read(|tx| {
            tx.events_of_type(EventType::DeliveryReentered)?
                .all_or_corrupt()
        })
        .unwrap()
}

#[test]
fn a_second_enter_is_recorded_once_with_the_place_and_no_role_for_the_lead() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    no_ack(&rig);
    deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    let events = reentered(&rig);
    assert_eq!(events.len(), 1);
    assert_eq!(events.first().unwrap().role, None);
    assert_eq!(
        events.first().unwrap().data,
        json!({ "to": "lead", "place": place.id })
    );
}

#[test]
fn a_second_enter_to_a_member_is_recorded_with_its_role() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    let party = rig.add_member("api", "fake", "member-1");
    let place = rig.host.add_place(
        "api",
        PlaceState::Idle,
        Occupant {
            session: Some(sid("member-1")),
            pids: vec![],
        },
    );
    rig.harness.add_session(
        sid("member-1"),
        vec![Pid::new(200)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.claim(200, &place.id);
    rig.host.set_screen(&place, "ready\n> ");
    no_ack(&rig);
    deliver_line(&mut rig.ctx(), &party, &one_line("hello there", 80));
    let events = reentered(&rig);
    assert_eq!(events.len(), 1);
    assert_eq!(events.first().unwrap().role, Some(role("api")));
    assert_eq!(
        events.first().unwrap().data,
        json!({ "to": "api", "place": place.id })
    );
}

#[test]
fn no_second_enter_writes_no_event() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    no_ack(&rig);
    rig.harness.set_detects_pending(false);
    deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert!(reentered(&rig).is_empty());
}
