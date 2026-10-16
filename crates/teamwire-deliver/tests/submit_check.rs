//! A host that gives no acknowledgement (tmux) gets one look at the screen
//! after the submit. A line still in the input box gets one more Enter.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use std::time::Duration;

use teamwire_core::{Key, SubmitAck, one_line};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeHost, PLAIN_TRUST_TEXT};
use teamwire_store::Party;

use rig::Rig;

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
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello there"]);
    assert_eq!(rig.host.keys_sent(&place), [Key::Enter]);
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
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    taken.join().unwrap();
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
    deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    opened.join().unwrap();
    assert!(rig.host.keys_sent(&place).is_empty());
}

#[test]
fn a_host_that_acknowledges_the_submit_is_not_checked() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello there", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert!(rig.host.keys_sent(&place).is_empty());
}
