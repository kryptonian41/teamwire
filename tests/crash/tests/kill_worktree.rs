//! `kill -9` of `teamwire launch` right after `git worktree add`, and of
//! `teamwire close` right after `git worktree remove`: the worktree and its
//! branch stay findable, and `close` finishes (LCH-3, `docs/failure-modes.md`
//! section 2).
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::{
    Identity, Rig, assert_closed_for_good, assert_integrity, close_until_done, git, stop_agents,
    temp_repo, wait_idle, worktree_spec,
};
use teamwire_core::Role;

fn wt(store: &teamwire_store::Store) -> Option<teamwire_core::Member> {
    store
        .read(|tx| tx.member(&Role::parse("wt").unwrap()))
        .unwrap()
}

#[test]
fn kill9_right_after_worktree_add_leaves_a_member_close_can_find() {
    let mut rig = Rig::bare();
    let repo = temp_repo();
    let tree = repo.path().join(".worktrees/wt");
    let signal = rig.pause_after_git("worktree add");
    let mut running = rig.start_with_stdin(
        &["launch"],
        &Identity::Nobody,
        &worktree_spec(repo.path()),
    );
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    running.finish();
    rig.path_prefix = None;

    // The worktree exists, and the row that names it was written before it.
    assert!(tree.is_dir(), "git worktree add had finished");
    let store = rig.store();
    assert_integrity(&store);
    let member = wt(&store).expect("the member was recorded before the worktree");
    assert_eq!(member.repo.as_deref(), Some(repo.path()));
    assert_eq!(member.branch.as_deref(), Some("wt"));
    drop(store);

    // Close removes the worktree it finds, or refuses and names it.
    let out = rig.run(&["close", "alpha"], &Identity::Nobody);
    if out.code == Some(0) {
        assert!(!tree.exists(), "close left the worktree: {out:?}");
    } else {
        let said = format!("{}{}", out.stdout, out.stderr);
        assert!(said.contains(&tree.display().to_string()), "{out:?}");
        assert!(tree.is_dir(), "close refused and removed it: {out:?}");
    }
    stop_agents(&rig);
}

#[test]
fn kill9_right_after_worktree_remove_keeps_the_branch_and_a_rerun_finishes() {
    let mut rig = Rig::bare();
    let repo = temp_repo();
    let tree = repo.path().join(".worktrees/wt");
    let launched = rig
        .start_with_stdin(&["launch"], &Identity::Nobody, &worktree_spec(repo.path()))
        .finish();
    assert_eq!(launched.code, Some(0), "{launched:?}");
    wait_idle(&rig);
    // A commit of the member's own: the branch must keep it.
    std::fs::write(tree.join("a.txt"), "work\n").unwrap();
    git(&tree, &["add", "a.txt"]);
    git(&tree, &["commit", "-q", "-m", "member work"]);
    let tip = git(repo.path(), &["rev-parse", "refs/heads/wt"]);

    let signal = rig.pause_after_git("worktree remove");
    let mut running = rig.start(&["close", "alpha"], &Identity::Nobody);
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    running.finish();
    rig.path_prefix = None;

    assert!(!tree.exists(), "git worktree remove had finished");
    assert_eq!(git(repo.path(), &["rev-parse", "refs/heads/wt"]), tip);
    assert_integrity(&rig.store());

    close_until_done(&rig);
    assert_eq!(git(repo.path(), &["rev-parse", "refs/heads/wt"]), tip);
    assert_closed_for_good(&rig);
    stop_agents(&rig);
}
