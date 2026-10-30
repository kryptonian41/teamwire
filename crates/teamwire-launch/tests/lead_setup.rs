//! A new team is refused when the lead's harness says the lead setup it
//! needs is not loaded in the caller's session, as for a pi lead without
//! teamwire's lead package (ADR 0030). A team that already runs is not
//! checked again.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, codes, pane_member, team_spec};
use teamwire_launch::new_team_lead;

const WHY: &str = "run `teamwire install --harness pi` and start pi again";

#[test]
fn a_new_team_is_refused_before_anything_is_made() {
    let rig = Rig::new();
    rig.harness.set_lead_setup_missing(Some(WHY));
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["lead_setup_missing"]);
    assert_eq!(errors.first().unwrap().message, WHY);
    assert!(rig.home_is_empty());
}

#[test]
fn a_new_team_lead_is_refused() {
    let rig = Rig::new();
    rig.harness.set_lead_setup_missing(Some(WHY));
    let error = new_team_lead(&rig.env()).unwrap_err();
    assert_eq!(error.code, "lead_setup_missing");
}

#[test]
fn a_member_added_to_a_running_team_is_not_refused() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    rig.launch("demo", plan);
    rig.harness.set_lead_setup_missing(Some(WHY));
    let more = team_spec("demo", &[pane_member("b", "Beta", rig.work.path())]);
    assert!(rig.validate(&more, true).is_ok());
}
