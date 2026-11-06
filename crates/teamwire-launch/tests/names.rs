//! A member whose address is already live in the host is refused.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, codes, pane_member, team_spec};
use teamwire_core::HostErrorCode;
use teamwire_fakes::Fault;

#[test]
fn name_already_live_in_host_refused() {
    let rig = Rig::new();
    rig.host.add_foreign_agent("tw_demo_a");
    let spec = team_spec(
        "demo",
        &[
            pane_member("a", "Alpha", rig.work.path()),
            pane_member("b", "Beta", rig.work.path()),
        ],
    );
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["name_taken"]);
    assert_eq!(errors.first().and_then(|e| e.role.as_deref()), Some("a"));
    assert!(rig.home_is_empty());
    assert_eq!(rig.host.log().count_named("spawn"), 0);
}

#[test]
fn an_unreadable_agent_list_refuses_the_member() {
    let rig = Rig::new();
    rig.host.inject_always(
        "live_agent_names",
        Fault::Fail(teamwire_core::HostError::new(HostErrorCode::Other, "boom")),
    );
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["host_listing_failed"]);
    assert_eq!(errors.first().and_then(|e| e.role.as_deref()), Some("a"));
    assert!(rig.home_is_empty());
    assert_eq!(rig.host.log().count_named("spawn"), 0);
}

#[test]
fn a_name_taken_answer_from_the_host_at_spawn_refuses_only_that_member() {
    use teamwire_launch::LaunchOutcome;
    let rig = Rig::new();
    let spec = team_spec(
        "demo",
        &[
            pane_member("a", "Alpha", rig.work.path()),
            pane_member("b", "Beta", rig.work.path()),
        ],
    );
    let plan = rig.validate(&spec, false).unwrap();
    // The name goes live between the check and the start.
    rig.host.add_foreign_agent("tw_demo_a");
    let (report, _store) = rig.launch("demo", plan);
    let outcomes: Vec<&LaunchOutcome> = report.members.iter().map(|m| &m.outcome).collect();
    assert!(matches!(outcomes.first(), Some(LaunchOutcome::Refused(_))));
    assert_eq!(outcomes.get(1).copied(), Some(&LaunchOutcome::Started));
}

#[test]
fn a_member_named_user_is_refused_because_the_name_is_the_human_users() {
    let rig = Rig::new();
    let spec = team_spec(
        "demo",
        &[
            pane_member("user", "User", rig.work.path()),
            pane_member("b", "Beta", rig.work.path()),
        ],
    );
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["reserved_role"]);
    assert_eq!(errors.first().and_then(|e| e.role.as_deref()), Some("user"));
    assert!(rig.home_is_empty());
    assert_eq!(rig.host.log().count_named("spawn"), 0);
}
