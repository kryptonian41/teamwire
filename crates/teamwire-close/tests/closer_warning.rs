//! The warning of a whole-team close while the team's code has had no
//! closer. Leads closed teams with no closer when nothing said so.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a member that writes files in its worktree"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, git};
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close};
use teamwire_core::{Clock, CompletionStatus, FinalSource, MemberKind, Role};
use teamwire_store::NewCompletion;

const WARNING: &str = "no closer reported on team alpha, and 1 implementer branch(es) have \
                       commits; add one from teamwire preset closer, or close anyway if the \
                       user said so.";

fn commit(rig: &Rig, role: &str) {
    let cwd = rig.member(role).cwd;
    std::fs::write(cwd.join(format!("{role}.rs")), "fn a() {}\n").unwrap();
    git(&cwd, &["add", "-A"]);
    git(&cwd, &["commit", "-q", "-m", "work"]);
}

fn report_done(rig: &mut Rig, role: &str) {
    let now = rig.clock.now();
    let generation = rig.member(role).generation;
    rig.store
        .write(|tx| {
            tx.insert_completion(
                now,
                &NewCompletion {
                    role: Role::parse(role).unwrap(),
                    generation,
                    status: CompletionStatus::Done,
                    summary: "checked".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
        })
        .unwrap();
}

#[test]
fn a_close_with_committed_implementers_and_no_closer_report_warns_and_goes_on() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    commit(&rig, "api");
    // An implementer with no commit of its own is not counted.
    rig.add_worktree_member("web", MemberKind::Implementer);
    // A closer that has not reported yet is no closer report.
    rig.add_pane_member("closer", MemberKind::Closer);

    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert_eq!(plan.closer_warning.as_deref(), Some(WARNING));
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.closer_warning.as_deref(), Some(WARNING));
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty(), "{:?}", report.refusals);
    assert!(!rig.team_dir().exists(), "the close went on");
}

#[test]
fn a_closer_report_or_no_committed_implementer_means_no_warning() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    commit(&rig, "api");
    rig.add_pane_member("closer", MemberKind::Closer);
    report_done(&mut rig, "closer");
    assert_eq!(
        plan_close(&rig.ctx(), CloseTarget::Team).closer_warning,
        None
    );

    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.add_pane_member("scout", MemberKind::Investigator);
    assert_eq!(
        plan_close(&rig.ctx(), CloseTarget::Team).closer_warning,
        None
    );
}
