//! The retry rules of ADR 0003 point 7: only a code that proves nothing was
//! submitted is retried, at most 3 tries in about 5 seconds.
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

use std::time::{Duration, Instant};

use teamwire_core::{Clock, DeliveryErrorCode, HostError, HostErrorCode, Timestamp, one_line};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeClock, Fault};
use teamwire_store::{Ctx, Party};

use rig::Rig;

fn fail(code: HostErrorCode) -> Fault {
    Fault::Fail(HostError::new(code, "scripted"))
}

fn error_code(outcome: &DeliveryOutcome) -> DeliveryErrorCode {
    match outcome {
        DeliveryOutcome::Undelivered(e) | DeliveryOutcome::Unconfirmed(e) => e.code,
        DeliveryOutcome::HandedOff => DeliveryErrorCode::HostOther,
    }
}

#[test]
fn line_is_typed_once_into_the_located_place() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello"]);
}

#[test]
fn busy_twice_then_success_is_handed_off() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    rig.host.inject("submit", fail(HostErrorCode::Busy), 1);
    rig.host.inject("submit", fail(HostErrorCode::NotReady), 1);
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.log().count_named("submit"), 3);
    assert_eq!(rig.host.submitted(&place), ["hello"]);
}

#[test]
fn busy_forever_stops_after_three_tries() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    rig.host.inject_always("submit", fail(HostErrorCode::Busy));
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert!(matches!(outcome, DeliveryOutcome::Undelivered(_)));
    assert_eq!(error_code(&outcome), DeliveryErrorCode::HostBusy);
    assert_eq!(rig.host.log().count_named("submit"), 3);
    assert!(rig.host.submitted(&place).is_empty());
}

#[test]
fn stall_is_unconfirmed_and_not_retried() {
    for code in [HostErrorCode::Stalled, HostErrorCode::Timeout] {
        let mut rig = Rig::new();
        let place = rig.healthy("lead-1", 100);
        // The host typed the text, then reported the stall.
        rig.host.inject(
            "submit",
            Fault::AcceptedButFail(HostError::new(code, "no state change")),
            1,
        );
        let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
        assert!(
            matches!(outcome, DeliveryOutcome::Unconfirmed(_)),
            "{outcome:?}"
        );
        assert_eq!(rig.host.log().count_named("submit"), 1);
        assert_eq!(
            rig.host.submitted(&place),
            ["hello"],
            "typed once, never again"
        );
    }
}

#[test]
fn unknown_error_not_retried() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    rig.host.inject_always(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Other, "something odd")),
    );
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
    let DeliveryOutcome::Undelivered(e) = outcome else {
        panic!("expected undelivered");
    };
    assert_eq!(e.code, DeliveryErrorCode::HostOther);
    assert_eq!(e.detail, "something odd");
    assert_eq!(rig.host.log().count_named("submit"), 1);
}

#[test]
fn other_refusals_are_not_retried() {
    for (code, want) in [
        (HostErrorCode::NotFound, DeliveryErrorCode::HostNotFound),
        (
            HostErrorCode::BadArgument,
            DeliveryErrorCode::HostBadArgument,
        ),
        (HostErrorCode::Unsupported, DeliveryErrorCode::HostOther),
    ] {
        let mut rig = Rig::new();
        rig.healthy("lead-1", 100);
        rig.host.inject_always("submit", fail(code));
        let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
        assert_eq!(error_code(&outcome), want);
        assert_eq!(rig.host.log().count_named("submit"), 1);
    }
}

/// A clock that moves 3 seconds every time the code reads the monotonic time.
struct SteppingClock(FakeClock);

impl Clock for SteppingClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
    fn monotonic(&self) -> Instant {
        let at = self.0.monotonic();
        self.0.advance(Duration::from_secs(3));
        at
    }
}

#[test]
fn retries_stop_when_the_five_second_window_is_spent() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    rig.host.inject_always("submit", fail(HostErrorCode::Busy));
    let clock = SteppingClock(FakeClock::new());
    let ctx = Ctx {
        home: rig.home.path(),
        store: &mut rig.store,
        registry: &rig.registry,
        platform: &rig.platform,
        clock: &clock,
        config: &rig.config,
    };
    let outcome = deliver_line(&ctx, &Party::Lead, &one_line("hello", 80));
    assert_eq!(error_code(&outcome), DeliveryErrorCode::HostBusy);
    // Start at 0 s; the check after try 1 reads 3 s (retry), after try 2 reads 6 s (stop).
    assert_eq!(rig.host.log().count_named("submit"), 2);
}

#[test]
fn empty_line_is_never_submitted() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("  \n\t ", 80));
    assert_eq!(error_code(&outcome), DeliveryErrorCode::HostBadArgument);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn host_without_typing_is_no_host() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    let mut caps = teamwire_fakes::FakeHost::caps();
    caps.can_type = false;
    rig.host.set_caps(caps);
    let outcome = deliver_line(&rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(error_code(&outcome), DeliveryErrorCode::NoHost);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}
