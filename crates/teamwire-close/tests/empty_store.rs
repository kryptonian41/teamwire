//! A store that `teamwire launch` made and was killed before it wrote
//! anything: close deletes the directory, and only when the store holds no
//! team row and no member row.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a person who edits files in a worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, TEAM};
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close_with};
use teamwire_core::{MemberKind, TeamName};

fn named() -> CloseOpts {
    CloseOpts {
        team: Some(TeamName::parse(TEAM).unwrap()),
        ..CloseOpts::default()
    }
}

#[test]
fn a_store_nothing_was_written_to_is_closed_by_deleting_its_directory() {
    let mut rig = Rig::without_team_row();
    let dir = rig.team_dir();
    assert!(dir.join("team.db").is_file());

    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named());
    assert!(plan.refusals.is_empty(), "{plan:?}");
    assert_eq!(plan.steps.len(), 1, "{plan:?}");
    assert_eq!(plan.steps[0].step, TEAM);
    assert!(plan.steps[0].detail.starts_with("nothing was recorded; "));
    assert!(dir.join("team.db").is_file(), "a dry run changes nothing");

    let report = close(&mut rig.ctx(), CloseTarget::Team, named());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    assert_eq!(report.steps.len(), 1, "{report:?}");
    assert_eq!(report.steps[0].step, TEAM);
    assert_eq!(
        report.steps[0].detail,
        format!("nothing was recorded; deleted {}", dir.display())
    );
    assert!(report.archive.is_none());
    assert!(!dir.exists());
}

#[test]
fn a_member_row_without_a_team_row_is_refused_and_kept() {
    let mut rig = Rig::without_team_row();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let dir = rig.team_dir();

    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named());
    assert!(
        plan.refusals.iter().any(|r| r.contains("no team row")),
        "{plan:?}"
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, named());
    assert!(
        report.refusals.iter().any(|r| r.contains("no team row")),
        "{report:?}"
    );
    assert!(dir.join("team.db").is_file());
}

#[test]
fn an_unnamed_team_is_not_guessed_from_an_empty_store() {
    let mut rig = Rig::without_team_row();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(!report.refusals.is_empty(), "{report:?}");
    assert!(rig.team_dir().exists());
}
