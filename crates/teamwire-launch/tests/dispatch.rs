//! The first message is sent once, and never again after a stall or timeout.

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, member_row, pane_member, team_spec};
use serde_json::json;
use teamwire_core::{HostError, HostErrorCode, MemberState};
use teamwire_fakes::Fault;
use teamwire_launch::LaunchOutcome;

#[test]
fn stalled_dispatch_not_resent() {
    let rig = Rig::new();
    rig.host.inject(
        "submit",
        Fault::AcceptedButFail(HostError::new(
            HostErrorCode::Stalled,
            "agent_prompt_stalled",
        )),
        u32::MAX,
    );
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    assert_eq!(
        rig.host.log().count_named("submit"),
        1,
        "one submit call only"
    );
    assert_eq!(member_row(&store, "a").unwrap().state, MemberState::Started);
    let warning = report
        .warnings
        .iter()
        .find(|w| w.starts_with("a:"))
        .unwrap();
    assert!(warning.contains("not resent"), "{warning}");
    assert!(
        warning.contains("teamwire send demo --to a --type note --text"),
        "{warning}"
    );
}

#[test]
fn a_timeout_on_dispatch_is_not_resent_either() {
    let rig = Rig::new();
    rig.host.inject("submit", Fault::Hang, u32::MAX);
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    assert_eq!(rig.host.log().count_named("submit"), 1);
}

#[test]
fn the_first_message_points_at_the_brief_and_is_one_line() {
    let rig = Rig::new();
    let (spec, repo) = common::one_worktree_team(&rig, "demo");
    let plan = rig.validate(&spec, false).unwrap();
    let (_, store) = rig.launch("demo", plan);
    let place = member_row(&store, "api").unwrap().place.unwrap();
    let sent = rig.host.submitted(&place);
    assert_eq!(sent.len(), 1);
    let line = sent.first().unwrap();
    assert!(line.contains("TASK.md") && !line.contains('\n'));
    assert!(line.contains(&repo.join(".worktrees/api/TASK.md").display().to_string()));
}

#[test]
fn dispatch_false_starts_the_member_idle() {
    let rig = Rig::new();
    let member = common::with(
        pane_member("a", "Alpha", rig.work.path()),
        "dispatch",
        json!(false),
    );
    let plan = rig.validate(&team_spec("demo", &[member]), false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    assert_eq!(rig.host.log().count_named("submit"), 0);
}
