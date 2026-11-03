//! The close half of a handoff: the busy and uncommitted-work checks, and
//! the place close that leaves the member's row, worktree and branch alone.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a member that edits files in its worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, git};
use teamwire_close::{close_handoff_place, handoff_refusals};
use teamwire_core::{Host, MemberKind, MemberState, Occupant, Pid, PlaceState, TeamName};

fn refusals(rig: &mut Rig, name: &str) -> Vec<String> {
    let member = rig.member(name);
    handoff_refusals(&rig.ctx(), &member)
}

#[test]
fn an_idle_clean_implementer_may_hand_off_with_no_verify_and_a_closer_still_at_work() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.add_pane_member("closer", MemberKind::Closer);
    assert_eq!(refusals(&mut rig, "api"), Vec::<String>::new());
}

#[test]
fn a_member_whose_session_ended_is_not_busy() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.set_place_state("api", PlaceState::Gone);
    assert_eq!(refusals(&mut rig, "api"), Vec::<String>::new());
}

#[test]
fn a_running_turn_a_dialog_and_uncommitted_work_are_refused() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.set_place_state("api", PlaceState::Working);
    let busy = refusals(&mut rig, "api");
    let [busy] = busy.as_slice() else {
        unreachable!("one refusal: {busy:?}");
    };
    assert!(busy.contains("api is busy"), "{busy}");
    assert!(!busy.contains("--stop"), "a handoff never stops a turn");

    rig.set_place_state("api", PlaceState::Blocked);
    let dialog = refusals(&mut rig, "api");
    assert!(dialog.iter().any(|r| r.contains("dialog")), "{dialog:?}");

    rig.set_place_state("api", PlaceState::Idle);
    std::fs::write(rig.member("api").cwd.join("half.rs"), "fn f() {}\n").unwrap();
    let dirty = refusals(&mut rig, "api");
    let [dirty] = dirty.as_slice() else {
        unreachable!("one refusal: {dirty:?}");
    };
    assert!(dirty.contains("1 uncommitted path(s)"), "{dirty}");
}

#[test]
fn closing_the_place_keeps_the_row_the_worktree_and_the_branch() {
    let mut rig = Rig::new();
    let before = rig.add_worktree_member("api", MemberKind::Implementer);
    let place = rig.place_of("api");
    let team = TeamName::parse(rig::TEAM).unwrap();
    let said = close_handoff_place(&rig.ctx(), &team, &before).unwrap();
    assert_eq!(said, Some(format!("api's place {} closed", place.id)));
    assert!(
        rig.host.state(&place, std::time::Duration::ZERO).is_err(),
        "the place is gone"
    );
    let after = rig.member("api");
    assert_eq!(after.state, MemberState::Started);
    assert_eq!(after.place, before.place, "the place stays recorded");
    assert!(after.cwd.is_dir());
    assert_eq!(
        git(&rig.repo, &["branch", "--list", "br-api"]).trim(),
        "+ br-api",
        "the branch stays checked out in the worktree"
    );
}

#[test]
fn a_pane_members_place_close_waits_for_its_processes_too() {
    let mut rig = Rig::new();
    let member = rig.add_pane_member("scout", MemberKind::Investigator);
    let place = rig.place_of("scout");
    let old = Pid::new(7011);
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![old],
        },
    );
    rig.platform.inner.set_alive(old, true);
    rig.config.close_exit_wait_ms = 100;
    let team = TeamName::parse(rig::TEAM).unwrap();
    let why = close_handoff_place(&rig.ctx(), &team, &member).unwrap_err();
    assert!(why.contains("7011"), "{why}");
    assert!(why.contains("no new session opens while they run"), "{why}");
}
