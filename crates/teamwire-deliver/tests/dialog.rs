//! ADR 0006 point 3a: nothing is typed into a session that shows a dialog.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::panic,
    reason = "a helper that panics on the wrong outcome fails the test, which is the intended result"
)]

#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{
    DeliveryErrorCode, HostError, HostErrorCode, Liveness, Pid, PlaceState, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeHost, Fault, PLAIN_TRUST_TEXT};
use teamwire_store::Party;

use rig::{Rig, sid};

fn refused_with(outcome: DeliveryOutcome) -> DeliveryErrorCode {
    match outcome {
        DeliveryOutcome::Undelivered(e) => e.code,
        other @ (DeliveryOutcome::HandedOff | DeliveryOutcome::Unconfirmed(_)) => {
            panic!("expected undelivered, got {other:?}")
        }
    }
}

fn unguarded(rig: &Rig) {
    let mut caps = FakeHost::caps();
    caps.guards_dialogs = false;
    rig.host.set_caps(caps);
}

#[test]
fn no_enter_into_a_dialog_on_unguarded_host() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    unguarded(&rig);
    rig.host.set_screen(&place, PLAIN_TRUST_TEXT);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("approve?", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::NotAtInputPrompt);
    assert_eq!(rig.host.log().count_named("submit"), 0);
    assert!(rig.host.submitted(&place).is_empty());
    assert!(rig.host.keys_sent(&place).is_empty(), "no Enter");
}

#[test]
fn unknown_screen_on_unguarded_host_is_not_typed_into() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    unguarded(&rig);
    rig.host.set_screen(&place, "compiling...");
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::NotAtInputPrompt);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn input_prompt_on_unguarded_host_is_typed_into() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    unguarded(&rig);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello"]);
}

#[test]
fn blocked_session_refused_before_submit() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    rig.host.set_state(&place, PlaceState::Blocked);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::SessionBlocked);
    assert_eq!(rig.host.log().count_named("submit"), 0);
    assert!(rig.host.submitted(&place).is_empty());
}

#[test]
fn harness_reporting_blocked_refuses_before_submit() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    rig.harness.add_session(
        sid("lead-1"),
        vec![Pid::new(100)],
        Liveness::Alive(PlaceState::Blocked),
    );
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::SessionBlocked);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn host_refusing_a_dialog_at_submit_is_a_blocked_session() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Blocked, "agent_blocked")),
        1,
    );
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::SessionBlocked);
    assert_eq!(rig.host.log().count_named("submit"), 1);
}

#[test]
fn an_unreadable_screen_is_read_again_and_typed_into_once_it_shows_the_prompt() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    unguarded(&rig);
    rig.host.set_screen(&place, "redrawing...");
    let host = rig.host.clone();
    let later = place.clone();
    let redraw = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        host.set_screen(&later, "ready\n> ");
    });
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    redraw.join().unwrap();
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello"]);
}

#[test]
fn a_screen_that_stays_unreadable_is_read_three_times_then_refused() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    unguarded(&rig);
    rig.host.set_screen(&place, "compiling...");
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::NotAtInputPrompt);
    assert_eq!(rig.host.log().count_named("read_tail"), 3);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn a_dialog_is_refused_at_once_without_a_second_read() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    unguarded(&rig);
    rig.host.set_screen(&place, PLAIN_TRUST_TEXT);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(refused_with(outcome), DeliveryErrorCode::NotAtInputPrompt);
    assert_eq!(rig.host.log().count_named("read_tail"), 1);
}
