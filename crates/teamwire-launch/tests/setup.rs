//! Setup runs before the agent exists; a failed setup undoes the member.

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, git, member_row, one_worktree_team, role_events};
use serde_json::json;
use teamwire_core::MemberState;
use teamwire_launch::LaunchOutcome;

#[test]
fn failed_setup_undoes_and_role_reusable() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let failing = common::with(spec.clone(), "setup", json!("echo boom; exit 3"));
    let plan = rig.validate(&failing, false).unwrap();
    let (report, store) = rig.launch("demo", plan);

    let outcome = &report.members.first().unwrap().outcome;
    assert!(matches!(outcome, LaunchOutcome::Failed(_)), "{report:?}");
    let reason = format!("{outcome:?}");
    assert!(reason.contains("setup exited 3"), "{reason}");
    assert!(
        reason.contains("worktree") && reason.contains("removed"),
        "{reason}"
    );
    assert!(reason.contains("--add"), "{reason}");
    // Undone: no checkout, no branch, no agent; the log is kept.
    assert!(!repo.join(".worktrees/api").exists());
    assert!(!git(&repo, &["worktree", "list", "--porcelain"]).contains(".worktrees/api"));
    assert_eq!(git(&repo, &["branch", "--list", "api"]), "");
    assert_eq!(rig.host.log().count_named("spawn"), 0);
    let log = std::fs::read_to_string(rig.home.path().join("teams/demo/setup/api.log")).unwrap();
    assert!(log.contains("boom") && log.contains("[exit 3]"));
    let row = member_row(&store, "api").unwrap();
    assert_eq!(row.state, MemberState::Failed);
    let events = role_events(&store, "api");
    assert_eq!(events.last().unwrap().0, "member.failed");
    // A metric label comes from the code, never from the free text (M8).
    let code = events.last().unwrap().1["reason_code"].as_str().unwrap();
    assert_eq!(code, "setup");
    assert!(
        (1..=32).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
        "{code}"
    );
    drop(store);

    // The same role, launched again with a corrected spec.
    let plan = rig.validate(&spec, true).unwrap();
    assert!(plan.members.first().unwrap().relaunch);
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let kinds: Vec<String> = role_events(&store, "api")
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        kinds,
        [
            "member.creating",
            "member.failed",
            "member.replaced",
            "member.creating",
            "member.started"
        ]
    );
    assert!(repo.join(".worktrees/api/README.md").is_file());
    assert_eq!(
        member_row(&store, "api").unwrap().state,
        MemberState::Started
    );
}

#[test]
fn setup_gets_the_directory_as_first_argument_and_the_team_variables() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let script = r#"printf '%s|%s|%s|%s|%s' "$1" "$TEAMWIRE_TEAM" "$TEAMWIRE_ROLE" "$TEAMWIRE_HOME" "$TEAMWIRE_CHILD_CWD" > setup-out.txt; pwd >> setup-out.txt"#;
    let spec = common::with(spec, "setup", json!(script));
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let dir = repo.join(".worktrees/api");
    let out = std::fs::read_to_string(dir.join("setup-out.txt")).unwrap();
    let want = format!(
        "{d}|demo|api|{h}|{d}",
        d = dir.display(),
        h = rig.home.path().display()
    );
    assert!(out.starts_with(&want), "{out} vs {want}");
}

#[test]
fn a_pane_member_does_not_get_the_team_setup() {
    let rig = Rig::new();
    let spec = common::team_spec(
        "demo",
        &[common::pane_member("a", "Alpha", rig.work.path())],
    );
    let spec = common::with(spec, "setup", json!("exit 9"));
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    assert!(
        !rig.home.path().join("teams/demo/setup/a.log").exists(),
        "setup never ran"
    );
}

#[test]
fn failed_setup_that_wrote_a_file_reports_leftover_worktree() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let failing = common::with(spec.clone(), "setup", json!("touch made.txt; exit 3"));
    let plan = rig.validate(&failing, false).unwrap();
    let (report, store) = rig.launch("demo", plan);

    let outcome = &report.members.first().unwrap().outcome;
    let reason = format!("{outcome:?}");
    assert!(reason.contains("setup exited 3"), "{reason}");
    assert!(
        reason.contains("made.txt"),
        "the leftover path is named: {reason}"
    );
    assert!(
        reason.contains("git worktree remove"),
        "the manual step: {reason}"
    );
    assert!(
        !reason.contains("--add"),
        "no advice that cannot work: {reason}"
    );
    assert!(
        repo.join(".worktrees/api/made.txt").is_file(),
        "nothing was forced"
    );
    assert_eq!(
        member_row(&store, "api").unwrap().state,
        MemberState::Failed
    );
    drop(store);

    // The same role again: the worktree is reused, not refused as `path_exists`.
    let plan = rig.validate(&spec, true).unwrap();
    assert!(
        plan.members
            .first()
            .unwrap()
            .worktree
            .as_ref()
            .unwrap()
            .reuse
    );
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started,
        "{report:?}"
    );
    assert_eq!(
        member_row(&store, "api").unwrap().state,
        MemberState::Started
    );
}

#[test]
fn a_modified_tracked_file_is_named_in_full() {
    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let failing = common::with(spec, "setup", json!("echo more >> README.md; exit 3"));
    let plan = rig.validate(&failing, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    let reason = format!("{:?}", report.members.first().unwrap().outcome);
    assert!(reason.contains("still holds README.md"), "{reason}");
}
