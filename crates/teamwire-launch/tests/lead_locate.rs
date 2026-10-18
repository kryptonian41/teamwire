//! A new team whose lead cannot read `teamwire watch` is refused before
//! anything is created when teamwire cannot find the lead's pane
//! (ADR 0023 point 9).
#![expect(
    clippy::unwrap_used,
    reason = "a helper in an integration-test file unwraps; a failure there fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, codes, pane_member, team_spec};
use teamwire_core::{Liveness, Occupant, Pid, PlaceState, SessionId};
use teamwire_fakes::FakeHarness;

fn rig_without_watch_reading() -> Rig {
    let rig = Rig::new();
    let mut caps = FakeHarness::caps();
    caps.reads_watch_lines = false;
    rig.harness.set_caps(caps);
    rig
}

#[test]
fn a_lead_that_cannot_be_found_is_refused_before_anything_is_made() {
    let rig = rig_without_watch_reading();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["lead_not_locatable"]);
    let message = &errors.first().unwrap().message;
    assert!(message.contains("codex --no-daemon"), "{message}");
    assert!(rig.home_is_empty());
}

#[test]
fn a_lead_found_in_its_pane_passes() {
    let rig = rig_without_watch_reading();
    let session = SessionId::parse("lead-session").unwrap();
    let place = rig.host.add_place(
        "lead-pane",
        PlaceState::Idle,
        Occupant {
            session: Some(session.clone()),
            pids: vec![],
        },
    );
    rig.harness.add_session(
        session,
        vec![Pid::new(500)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.platform.inner.set_alive(Pid::new(500), true);
    rig.platform
        .inner
        .set_env(Pid::new(500), "FAKE_PLACE_ID", &place.id);
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    assert!(rig.validate(&spec, false).is_ok());
}

#[test]
fn a_lead_that_reads_watch_lines_is_not_checked() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    assert!(rig.validate(&spec, false).is_ok());
}

fn launched_watch_command(rig: &Rig) -> String {
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    rig.launch("demo", plan).0.watch_command
}

#[test]
fn the_watch_command_follows_what_the_leads_harness_reads() {
    assert_eq!(launched_watch_command(&Rig::new()), "teamwire watch demo");
    let rig = rig_without_watch_reading();
    let session = SessionId::parse("lead-session").unwrap();
    let place = rig.host.add_place(
        "lead-pane",
        PlaceState::Idle,
        Occupant {
            session: Some(session.clone()),
            pids: vec![],
        },
    );
    rig.harness.add_session(
        session,
        vec![Pid::new(500)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.platform.inner.set_alive(Pid::new(500), true);
    rig.platform
        .inner
        .set_env(Pid::new(500), "FAKE_PLACE_ID", &place.id);
    assert_eq!(
        launched_watch_command(&rig),
        "nohup teamwire watch demo --deliver >/dev/null 2>&1 &"
    );
}
