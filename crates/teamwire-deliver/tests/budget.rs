//! ADR 0007 point 6a: inside the turn-end hook every call gets the smaller of
//! its own bound and the time left, and a spent budget types nothing.
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

use std::sync::{Arc, Mutex};
use std::time::Duration;

use teamwire_core::{
    Clock, DeliveryErrorCode, Host, HostCaps, HostError, HostId, Key, Occupant, OneLine, Place,
    PlaceState, Registry, SpawnRequest, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line_within};
use teamwire_fakes::{FakeClock, FakeHost};
use teamwire_store::Party;

use rig::Rig;

/// A host whose every call takes `per_call` on the shared clock and which
/// remembers the timeout each call was given.
struct SlowHost {
    inner: Arc<FakeHost>,
    clock: Arc<FakeClock>,
    per_call: Duration,
    timeouts: Mutex<Vec<Duration>>,
}

impl SlowHost {
    fn took(&self, timeout: Duration) {
        self.timeouts.lock().unwrap().push(timeout);
        self.clock.advance(self.per_call);
    }

    fn given(&self) -> Vec<Duration> {
        self.timeouts.lock().unwrap().clone()
    }
}

impl Host for SlowHost {
    fn id(&self) -> HostId {
        self.inner.id()
    }
    fn capabilities(&self) -> HostCaps {
        self.inner.capabilities()
    }
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
        self.took(timeout);
        self.inner.spawn(req, timeout)
    }
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError> {
        self.took(timeout);
        self.inner.submit(place, text, timeout)
    }
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        self.took(timeout);
        self.inner.state(place, timeout)
    }
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
        self.took(timeout);
        self.inner.occupant(place, timeout)
    }
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        self.took(timeout);
        self.inner.read_tail(place, lines, timeout)
    }
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        self.took(timeout);
        self.inner.send_keys(place, keys, timeout)
    }
    fn close(&self, place: &Place, timeout: Duration) -> Result<(), HostError> {
        self.took(timeout);
        self.inner.close(place, timeout)
    }
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        self.took(timeout);
        self.inner.list(timeout)
    }
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        self.took(timeout);
        self.inner.live_agent_names(timeout)
    }
    fn place_env_var(&self) -> Option<&'static str> {
        self.inner.place_env_var()
    }
}

/// A rig whose registry holds a `SlowHost` in place of the plain fake.
fn slow_rig(per_call: Duration) -> (Rig, Arc<SlowHost>) {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    let slow = Arc::new(SlowHost {
        inner: rig.host.clone(),
        clock: rig.clock.clone(),
        per_call,
        timeouts: Mutex::new(Vec::new()),
    });
    let mut registry = Registry::new();
    registry.add_harness(rig.harness.clone()).unwrap();
    registry.add_host(slow.clone()).unwrap();
    rig.registry = registry;
    (rig, slow)
}

fn refused_with(outcome: &DeliveryOutcome) -> DeliveryErrorCode {
    match outcome {
        DeliveryOutcome::Undelivered(e) => e.code,
        DeliveryOutcome::HandedOff | DeliveryOutcome::Unconfirmed(_) => {
            panic!("expected undelivered, got {outcome:?}")
        }
    }
}

#[test]
fn budget_runs_out_between_calls_and_nothing_is_typed() {
    let (mut rig, slow) = slow_rig(Duration::from_secs(5));
    let deadline = rig.clock.monotonic() + Duration::from_secs(12);
    let outcome = deliver_line_within(
        &mut rig.ctx(),
        &Party::Lead,
        &one_line("hello", 80),
        deadline,
    );
    assert_eq!(refused_with(&outcome), DeliveryErrorCode::BudgetExhausted);
    // occupant 5 s (7 s left), second occupant 5 s (2 s left), state gets the 2 s left.
    let secs = |n| Duration::from_secs(n);
    assert_eq!(slow.given(), [secs(5), secs(5), secs(2)]);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn spent_budget_makes_no_call_at_all() {
    let (mut rig, slow) = slow_rig(Duration::ZERO);
    let deadline = rig.clock.monotonic();
    let outcome = deliver_line_within(
        &mut rig.ctx(),
        &Party::Lead,
        &one_line("hello", 80),
        deadline,
    );
    assert_eq!(refused_with(&outcome), DeliveryErrorCode::BudgetExhausted);
    assert!(slow.given().is_empty());
    assert_eq!(rig.harness.log().count_named("processes_for_session"), 0);
}

#[test]
fn every_call_is_bounded_by_the_time_left() {
    let (mut rig, slow) = slow_rig(Duration::ZERO);
    let deadline = rig.clock.monotonic() + Duration::from_secs(3);
    let outcome = deliver_line_within(
        &mut rig.ctx(),
        &Party::Lead,
        &one_line("hello", 80),
        deadline,
    );
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    let given = slow.given();
    assert_eq!(given.len(), 4, "occupant, occupant, state, submit");
    assert!(
        given.iter().all(|t| *t == Duration::from_secs(3)),
        "{given:?}"
    );
}

#[test]
fn roomy_deadline_keeps_the_five_second_bound() {
    let (mut rig, slow) = slow_rig(Duration::ZERO);
    let deadline = rig.clock.monotonic() + Duration::from_secs(40);
    let outcome = deliver_line_within(
        &mut rig.ctx(),
        &Party::Lead,
        &one_line("hello", 80),
        deadline,
    );
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert!(slow.given().iter().all(|t| *t == Duration::from_secs(5)));
}
