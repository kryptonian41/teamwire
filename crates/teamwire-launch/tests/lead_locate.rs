//! The watch command launch names for a new team's lead, and the lead that
//! `new_team_lead` finds for a team that does not exist yet.
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

use common::{Rig, pane_member, team_spec};
use teamwire_core::SessionId;
use teamwire_core::harness::WatchStart;
use teamwire_fakes::FakeHarness;
use teamwire_launch::new_team_lead;

fn launched_watch_command(rig: &Rig) -> String {
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    rig.launch("demo", plan).0.watch_command
}

#[test]
fn the_watch_command_follows_what_the_leads_harness_reads() {
    assert_eq!(launched_watch_command(&Rig::new()), "teamwire watch demo");
}

#[test]
fn a_lead_that_starts_the_watch_with_a_tool_is_told_the_tool() {
    let rig = Rig::new();
    let mut caps = FakeHarness::caps();
    caps.watch_start = WatchStart::ExtensionTool;
    rig.harness.set_caps(caps);
    assert_eq!(
        launched_watch_command(&rig),
        "call the teamwire_watch tool with team demo"
    );
}

#[test]
fn a_new_team_lead_is_the_callers_session() {
    let rig = Rig::new();
    let lead = new_team_lead(&rig.env()).unwrap();
    assert_eq!(
        lead.session.map(|(s, _)| s),
        Some(SessionId::parse("lead-session").unwrap())
    );
}
