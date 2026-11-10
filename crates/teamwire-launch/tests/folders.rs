//! The folders launch makes for a team's members before any of them starts:
//! `reports/` in the team directory, and each member's scratch folder outside
//! it.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, launch_one};

#[test]
fn launch_makes_the_reports_folder_before_the_first_member_writes() {
    let rig = Rig::new();
    let (report, _store) = launch_one(&rig, "demo", "a", false);
    assert!(
        matches!(
            report.members.first().map(|m| &m.outcome),
            Some(teamwire_launch::LaunchOutcome::Started)
        ),
        "{report:?}"
    );
    assert!(rig.home.path().join("teams/demo/reports").is_dir());
}

#[test]
fn launch_add_makes_the_reports_folder_when_it_is_missing() {
    let rig = Rig::new();
    drop(launch_one(&rig, "demo", "a", false));
    let reports = rig.home.path().join("teams/demo/reports");
    std::fs::remove_dir(&reports).unwrap();
    drop(launch_one(&rig, "demo", "b", true));
    assert!(reports.is_dir());
}
