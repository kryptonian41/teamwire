//! Launch writes a member's contract before its harness builds the launch.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;
#[path = "../../teamwire-msg/tests/support/hint_harness.rs"]
mod hint_harness;

use std::sync::Arc;

use common::{Rig, pane_member, team_spec};
use hint_harness::HintHarness;
use teamwire_core::Registry;

#[test]
fn the_contract_is_on_disk_before_the_harness_builds_its_launch() {
    let mut rig = Rig::new();
    let harness = HintHarness::new(Arc::clone(&rig.harness)).reading_contract();
    let mut registry = Registry::new();
    registry.add_harness(Arc::new(harness)).unwrap();
    registry.add_host(rig.host.clone()).unwrap();
    rig.registry = registry;
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        teamwire_launch::LaunchOutcome::Started
    );
}
