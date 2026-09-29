//! A lead and a member on different harnesses: each party is looked up and
//! typed into through its own adapters (ADP-12).
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/members.rs"]
mod members;
#[path = "support/rig.rs"]
mod rig;

use std::sync::Arc;

use teamwire_core::{
    DeliveryErrorCode, HarnessId, Liveness, Occupant, Pid, PlaceState, SessionKind, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line, locate};
use teamwire_fakes::FakeHarness;
use teamwire_store::Party;

use rig::{Rig, sid};

#[test]
fn lead_and_member_on_different_harnesses_route_per_party() {
    let mut rig = Rig::build(SessionKind::Interactive, "fake-a");
    let a = rig.harness.clone();
    let b = Arc::new(FakeHarness::with_id(HarnessId::from_static("fake-b")));
    rig.registry.add_harness(b.clone()).unwrap();
    let member = rig.add_member("api", "fake-b", "member-1");

    // The lead's session is known only to harness A, the member's only to B.
    let lead_place = rig.healthy("lead-1", 100);
    let member_place = rig.host.add_place(
        "api",
        PlaceState::Idle,
        Occupant {
            session: Some(sid("member-1")),
            pids: vec![],
        },
    );
    b.add_session(
        sid("member-1"),
        vec![Pid::new(200)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.claim(200, &member_place.id);
    rig.host.set_screen(&member_place, "ready\n> ");

    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), lead_place);
    assert_eq!(locate(&rig.ctx(), &member).unwrap(), member_place);
    assert_eq!(
        deliver_line(&rig.ctx(), &Party::Lead, &one_line("to lead", 80)),
        DeliveryOutcome::HandedOff
    );
    assert_eq!(
        deliver_line(&rig.ctx(), &member, &one_line("to member", 80)),
        DeliveryOutcome::HandedOff
    );
    assert_eq!(rig.host.submitted(&lead_place), ["to lead"]);
    assert_eq!(rig.host.submitted(&member_place), ["to member"]);

    let asked = |h: &FakeHarness| -> Vec<String> {
        h.log()
            .calls()
            .iter()
            .filter(|c| c.name == "processes_for_session")
            .map(|c| c.detail.clone())
            .collect()
    };
    assert!(asked(&a).iter().all(|s| s == "lead-1"), "{:?}", asked(&a));
    assert!(asked(&b).iter().all(|s| s == "member-1"), "{:?}", asked(&b));
    assert!(!asked(&a).is_empty() && !asked(&b).is_empty());
}

#[test]
fn unknown_member_harness_is_undelivered() {
    let mut rig = Rig::new();
    let member = rig.add_member("api", "not-in-this-binary", "member-1");
    rig.healthy("lead-1", 100);
    let outcome = deliver_line(&rig.ctx(), &member, &one_line("hello", 80));
    let DeliveryOutcome::Undelivered(e) = outcome else {
        unreachable!("an unknown adapter must not hand off");
    };
    assert_eq!(e.code, DeliveryErrorCode::UnknownAdapter);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}
