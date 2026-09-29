//! The sleep hold waits on the lead's harness process (SLP-1, SLP-2).

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, hold, launch_one, lead_at, seed_lead_team, team_hold};
use teamwire_core::Pid;
use teamwire_fakes::HoldEvent;

#[test]
fn hold_targets_harness_pid() {
    let rig = Rig::new();
    lead_at(&rig, 4242);
    let (report, store) = launch_one(&rig, "demo", "a", false);
    let events = rig.platform.inner.hold_events();
    assert_eq!(events.len(), 1);
    let started = match events.first() {
        Some(HoldEvent::Started { lead, hold }) => {
            assert_eq!(
                *lead,
                Pid::new(4242),
                "the hold waits on the harness process"
            );
            *hold
        }
        other => unreachable!("expected a started hold, got {other:?}"),
    };
    assert_eq!(team_hold(&store), Some(hold(started.get(), true)));
    assert!(report.warnings.iter().all(|w| !w.contains("SLEEP HOLD")));
}

#[test]
fn no_lead_process_found_says_so_loudly_and_records_no_hold() {
    let rig = Rig::new();
    let (report, store) = launch_one(&rig, "demo", "a", false);
    assert!(rig.platform.inner.hold_events().is_empty());
    assert_eq!(team_hold(&store), None);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.starts_with("NO SLEEP HOLD")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_hold_the_user_started_is_reused_and_not_owned() {
    let rig = Rig::new();
    lead_at(&rig, 4242);
    rig.platform
        .inner
        .add_external_hold(Pid::new(4242), Pid::new(777));
    let (_, store) = launch_one(&rig, "demo", "a", false);
    assert!(
        rig.platform.inner.hold_events().is_empty(),
        "nothing new was started"
    );
    assert_eq!(team_hold(&store), Some(hold(777, false)));
}

#[test]
fn a_hold_another_open_team_owns_is_reused_as_owned() {
    let rig = Rig::new();
    lead_at(&rig, 4242);
    rig.platform
        .inner
        .add_external_hold(Pid::new(4242), Pid::new(777));
    seed_lead_team(&rig, "other", Some(hold(777, true)));
    let (_, store) = launch_one(&rig, "demo", "a", false);
    assert_eq!(team_hold(&store), Some(hold(777, true)));
}

#[test]
fn adding_to_a_team_keeps_its_own_hold_and_starts_no_second() {
    let rig = Rig::new();
    lead_at(&rig, 4242);
    let (_, store) = launch_one(&rig, "demo", "a", false);
    let first = team_hold(&store).unwrap();
    drop(store);
    let (_, store) = launch_one(&rig, "demo", "b", true);
    assert_eq!(rig.platform.inner.hold_events().len(), 1);
    assert_eq!(team_hold(&store), Some(first));
    assert!(first.owned);
}

#[test]
fn a_platform_without_a_hold_says_so() {
    let rig = Rig::without_hold();
    lead_at(&rig, 4242);
    let (report, store) = launch_one(&rig, "demo", "a", false);
    assert_eq!(team_hold(&store), None);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("no sleep hold on this platform"))
    );
}

#[test]
fn a_failed_hold_start_is_a_warning_not_a_failure() {
    let rig = Rig::new();
    lead_at(&rig, 4242);
    rig.platform
        .inner
        .fail_hold_start(Some(teamwire_core::PlatformError::new(
            teamwire_core::PlatformErrorCode::Io,
            "no caffeinate",
        )));
    let (report, store) = launch_one(&rig, "demo", "a", false);
    assert_eq!(team_hold(&store), None);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("could not be started"))
    );
    assert_eq!(
        report.members.first().unwrap().outcome,
        teamwire_launch::LaunchOutcome::Started
    );
}
