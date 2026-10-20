//! Whole-team close of a worktree member whose HEAD is detached (CLS-1): its
//! place is closed right before its worktree is removed, inside
//! `remove_worktrees`, so nothing can commit between the last HEAD check and
//! the removal. A member on a branch keeps its place until `close_places`.

#[path = "support/rig.rs"]
pub mod rig;

use std::time::Duration;

use rig::{Rig, git};
use teamwire_close::{CloseOpts, CloseReport, CloseTarget, close, close_observed};
use teamwire_core::{Host, MemberKind, Occupant, Pid};

/// An implementer that detached its HEAD and committed there, so the commit
/// is on no branch, tag or remote ref. Returns the commit's sha.
fn detached_commit(rig: &mut Rig, role: &str) -> String {
    let member = rig.add_worktree_member(role, MemberKind::Implementer);
    git(&member.cwd, &["checkout", "-q", "--detach"]);
    commit(&member.cwd, "work on a detached head")
}

/// A commit in `cwd`, as the agent working there makes one. Returns its sha.
fn commit(cwd: &std::path::Path, message: &str) -> String {
    git(cwd, &["commit", "-q", "--allow-empty", "-m", message]);
    git(cwd, &["rev-parse", "HEAD"])
}

#[cfg(test)]
fn detail<'r>(report: &'r CloseReport, step: &str) -> &'r str {
    &report.steps.iter().find(|s| s.step == step).unwrap().detail
}

#[test]
fn a_detached_members_place_is_closed_in_remove_worktrees_before_its_worktree_goes() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    let api = rig.member("api");
    let place = rig.place_of("api");
    let host = rig.host.clone();
    let open_at_close_places = std::cell::Cell::new(None);
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "close_places" {
                open_at_close_places.set(Some(host.state(&place, Duration::ZERO).is_ok()));
            }
        },
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(
        open_at_close_places.get(),
        Some(false),
        "the place was already closed when close_places began"
    );
    // The notes follow the order of the work: the place, then the worktree.
    let removed = detail(&report, "remove_worktrees");
    let closed_at = removed.find(&format!("api's place {} closed", place.id));
    let gone_at = removed.find(&format!("api's worktree {} removed", api.cwd.display()));
    assert!(
        closed_at.is_some() && gone_at.is_some() && closed_at < gone_at,
        "{removed}"
    );
    assert!(!api.cwd.exists());
    assert_eq!(
        detail(&report, "close_places"),
        format!("api's place {} was already gone", place.id)
    );
}

#[test]
fn a_process_that_outlives_the_closed_place_keeps_the_worktree_until_it_ends() {
    let mut rig = Rig::new();
    let sha = detached_commit(&mut rig, "api");
    let api = rig.member("api");
    let place = rig.place_of("api");
    // A git command the hangup reached, still finishing after the place closed.
    let late = Pid::new(7001);
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![late],
        },
    );
    rig.platform.inner.set_alive(late, true);
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("remove_worktrees"),
        "{report:?}"
    );
    let why = detail(&report, "remove_worktrees");
    assert!(
        why.contains("7001") && why.contains("run close again"),
        "{why}"
    );
    assert!(
        rig.host.state(&place, Duration::ZERO).is_err(),
        "the place is closed"
    );
    assert!(
        api.cwd.is_dir(),
        "the worktree stays while the process runs"
    );

    // The process ends: a rerun removes the worktree and keeps the commit.
    rig.platform.inner.set_alive(late, false);
    let rerun = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(rerun.stopped_at, None, "{rerun:?}");
    assert!(!api.cwd.exists());
    assert_ne!(git(&rig.repo, &["for-each-ref", "--contains", &sha]), "");
}

#[test]
fn a_member_on_a_branch_keeps_its_place_until_close_places() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    rig.add_worktree_member("web", MemberKind::Implementer);
    let api = rig.place_of("api");
    let web = rig.place_of("web");
    let host = rig.host.clone();
    let open = std::cell::RefCell::new(Vec::new());
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            let web_open = host.state(&web, Duration::ZERO).is_ok();
            open.borrow_mut().push((step.to_owned(), web_open));
        },
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    let open_at = |step: &str| {
        open.borrow()
            .iter()
            .find(|(s, _)| s == step)
            .map(|(_, open)| *open)
    };
    assert_eq!(open_at("remove_worktrees"), Some(true));
    assert_eq!(open_at("close_places"), Some(true));
    assert_eq!(open_at("release_hold"), Some(false));
    assert_eq!(
        detail(&report, "close_places"),
        format!(
            "api's place {} was already gone; web's place {} closed",
            api.id, web.id
        )
    );
}

#[test]
fn a_commit_made_as_remove_worktrees_begins_is_kept_on_a_ref() {
    let mut rig = Rig::new();
    let first = detached_commit(&mut rig, "api");
    let cwd = rig.member("api").cwd;
    let last = std::cell::RefCell::new(String::new());
    // The agent's last act before its place closes.
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "remove_worktrees" {
                *last.borrow_mut() = commit(&cwd, "a last commit");
            }
        },
    );
    let last = last.into_inner();
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!cwd.exists());
    let rescue = format!(
        "teamwire/rescue/{}-api-{}",
        rig::TEAM,
        last.get(..7).unwrap()
    );
    assert_eq!(report.branches_kept, ["br-api".to_owned(), rescue.clone()]);
    assert_eq!(git(&rig.repo, &["rev-parse", &rescue]), last);
    for sha in [&first, &last] {
        assert_ne!(
            git(&rig.repo, &["for-each-ref", "--contains", sha]),
            "",
            "{sha} is on no ref"
        );
    }
}

#[test]
fn the_dry_run_names_each_detached_member_whose_place_closes_first() {
    let mut rig = Rig::new();
    detached_commit(&mut rig, "api");
    let web = rig.add_worktree_member("web", MemberKind::Implementer);
    let api = rig.member("api");
    let plan = teamwire_close::plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{plan:?}");
    let step = plan.steps.iter().find(|s| s.step == "remove_worktrees");
    assert_eq!(
        step.map(|s| s.detail.clone()),
        Some(format!(
            "git worktree remove, without force: api ({}), web ({}); api's HEAD is detached, so \
             its place is closed right before its worktree is removed",
            api.cwd.display(),
            web.cwd.display()
        )),
        "{plan:?}"
    );
}
