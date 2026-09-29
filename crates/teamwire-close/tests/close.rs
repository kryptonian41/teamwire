//! `plan_close` and `close`: the refusals that come before any step, then
//! the fixed order, the close lock and the re-checks under it.
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a person who edits files in a worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::Rig;
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close};
use teamwire_core::{EventType, Host, HostError, HostErrorCode, MemberKind, PlaceState, TeamState};
use teamwire_fakes::Fault;

fn asks(reasons: &[String], word: &str) -> bool {
    reasons.iter().any(|r| r.contains(word))
}

#[test]
fn busy_member_refuses() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.set_place_state("api", PlaceState::Working);

    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(asks(&plan.refusals, "api is busy"), "{:?}", plan.refusals);
    assert!(
        !asks(&plan.refusals, "scout"),
        "an idle member is not named: {:?}",
        plan.refusals
    );

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(asks(&report.refusals, "api is busy"), "{report:?}");
    assert!(
        report.steps.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    // Nothing changed: the team is open, both places and the worktree are there.
    assert_eq!(rig.team().state, TeamState::Open);
    assert_eq!(
        rig.host
            .state(&rig.place_of("scout"), std::time::Duration::ZERO),
        Ok(PlaceState::Idle)
    );
    assert!(rig.member("api").cwd.is_dir());
    assert!(!rig.archive_root().exists());
    assert_eq!(rig.events(EventType::CloseRefused).len(), 1);
    assert!(rig.events(EventType::TeamClosing).is_empty());
}

#[test]
fn a_dialog_waiting_for_an_answer_refuses_too() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Blocked);
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(asks(&plan.refusals, "dialog"), "{:?}", plan.refusals);
}

#[test]
fn unknown_busy_state_refuses() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.add_pane_member("rev", MemberKind::Reviewer);
    // One host that reports no state, one that fails outright.
    rig.set_place_state("scout", PlaceState::Unknown);
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(
        asks(&plan.refusals, "cannot tell whether scout is busy"),
        "{:?}",
        plan.refusals
    );

    rig.set_place_state("scout", PlaceState::Idle);
    rig.host.inject_always(
        "state",
        Fault::Fail(HostError::new(
            HostErrorCode::Other,
            "the herdr socket is gone",
        )),
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        asks(&report.refusals, "cannot tell whether scout is busy"),
        "{report:?}"
    );
    assert!(
        asks(&report.refusals, "cannot tell whether rev is busy"),
        "{report:?}"
    );
    assert!(asks(&report.refusals, "herdr socket is gone"), "{report:?}");
    assert_eq!(rig.team().state, TeamState::Open);
}

#[test]
fn a_place_the_host_no_longer_knows_is_not_busy() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.host.inject_always(
        "state",
        Fault::Fail(HostError::new(HostErrorCode::NotFound, "no such place")),
    );
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
}

#[test]
fn dirty_worktree_refuses() {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    // The brief's copy is not work: TASK.md never counts, tracked or not.
    std::fs::write(api.cwd.join("TASK.md"), "brief\n").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);

    std::fs::write(api.cwd.join("notes.txt"), "not committed\n").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(
        asks(&plan.refusals, "api has 1 uncommitted path"),
        "{:?}",
        plan.refusals
    );

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(asks(&report.refusals, "uncommitted"), "{report:?}");
    assert!(
        api.cwd.join("notes.txt").is_file(),
        "the uncommitted file is untouched"
    );
    assert_eq!(rig.team().state, TeamState::Open);
}

#[test]
fn git_that_cannot_say_refuses_rather_than_guess() {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    // Break the checkout's link to its repository: the directory is there,
    // but `git status` fails in it.
    std::fs::write(api.cwd.join(".git"), "gitdir: /nowhere\n").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(
        asks(
            &plan.refusals,
            "cannot tell whether api has uncommitted work"
        ),
        "{:?}",
        plan.refusals
    );
}

#[test]
fn a_removed_member_is_not_asked_about() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Working);
    let role = teamwire_core::Role::parse("scout").unwrap();
    let now = teamwire_core::Clock::now(&rig.clock);
    rig.store
        .write(|tx| tx.remove_member(now, &role, Some("removed"), &serde_json::json!({})))
        .unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
}
