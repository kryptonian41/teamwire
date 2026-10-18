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
    let code = events
        .last()
        .unwrap()
        .1
        .get("reason_code")
        .and_then(|v| v.as_str())
        .unwrap();
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

const MAKES_FILES: &str = "touch .setup-marker; mkdir -p cache/deep; touch cache/deep/x";

#[test]
fn files_setup_created_are_ignored_and_named() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!(MAKES_FILES)), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let dir = repo.join(".worktrees/api");
    // Close asks `git status --porcelain --untracked-files=normal` and
    // refuses a worktree that prints anything (teamwire-close/src/facts.rs).
    assert_eq!(
        git(&dir, &["status", "--porcelain", "--untracked-files=normal"]),
        ""
    );
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|l| l == "/.setup-marker"), "{exclude}");
    assert!(exclude.lines().any(|l| l == "/cache/deep/x"), "{exclude}");
    let note = report
        .warnings
        .iter()
        .find(|w| w.contains(".setup-marker"))
        .unwrap();
    assert!(
        note.contains("/cache/deep/x") && note.contains("info/exclude"),
        "{note}"
    );
}

#[test]
fn a_second_member_adds_no_duplicate_lines_and_old_lines_stay() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let exclude = repo.join(".git/info/exclude");
    common::write(&exclude, "# mine\n*.log");
    let spec = common::team_spec(
        "demo",
        &[
            common::worktree_member("api", "API", &repo),
            common::worktree_member("web", "Web", &repo),
        ],
    );
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!(MAKES_FILES)), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    assert!(
        report
            .members
            .iter()
            .all(|m| m.outcome == LaunchOutcome::Started),
        "{report:?}"
    );
    let text = std::fs::read_to_string(&exclude).unwrap();
    assert!(text.starts_with("# mine\n*.log\n"), "{text}");
    for line in ["/.setup-marker", "/cache/deep/x", "TASK.md"] {
        assert_eq!(text.lines().filter(|l| *l == line).count(), 1, "{text}");
    }
}

#[test]
fn setup_that_writes_nothing_changes_no_exclude_file() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    let before = std::fs::read_to_string(&exclude).ok();
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!("true")), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    assert!(
        report.warnings.iter().all(|w| !w.contains("setup created")),
        "{:?}",
        report.warnings
    );
    let after = std::fs::read_to_string(&exclude).unwrap();
    // Only the brief is added, as before.
    assert_eq!(after, format!("{}TASK.md\n", before.unwrap_or_default()));
}

#[test]
fn a_file_name_with_a_newline_is_named_and_never_written() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let script = "touch 'a\nb' 'sp ace' '#hash'";
    let (report, _store) = {
        let plan = rig
            .validate(&common::with(spec, "setup", json!(script)), false)
            .unwrap();
        rig.launch("demo", plan)
    };
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(
        !exclude.contains("a\nb") && !exclude.lines().any(|l| l == "/a" || l == "b"),
        "{exclude}"
    );
    assert!(exclude.lines().any(|l| l == "/sp ace"), "{exclude}");
    assert!(exclude.lines().any(|l| l == "/\\#hash"), "{exclude}");
    assert!(
        report.warnings.iter().any(|w| w.contains("a\\nb")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_file_written_later_in_a_setup_made_directory_stays_visible() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let spec = common::with(spec, "setup", json!("mkdir -p gen; touch gen/cache"));
    let plan = rig.validate(&spec, false).unwrap();
    let (_report, _store) = rig.launch("demo", plan);
    let dir = repo.join(".worktrees/api");
    common::write(&dir.join("gen/work.rs"), "fn main() {}\n");
    assert_eq!(
        git(&dir, &["status", "--porcelain", "--untracked-files=normal"]),
        "?? gen/"
    );
}

#[test]
fn a_member_that_fails_after_setup_takes_its_exclude_lines_back() {
    use teamwire_core::{HostError, HostErrorCode};
    use teamwire_fakes::Fault;
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    common::write(&exclude, "# mine\n*.log\n");
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::Other, "boom")),
        1,
    );
    let plan = rig
        .validate(&common::with(spec, "setup", json!("touch made.txt")), false)
        .unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert!(
        matches!(
            report.members.first().unwrap().outcome,
            LaunchOutcome::Failed(_)
        ),
        "{report:?}"
    );
    let text = std::fs::read_to_string(&exclude).unwrap();
    assert_eq!(text, "# mine\n*.log\n");
}

#[test]
fn the_exclude_note_agrees_in_number() {
    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let one = rig
        .validate(
            &common::with(spec.clone(), "setup", json!("touch a.txt")),
            false,
        )
        .unwrap();
    let (report, _store) = rig.launch("demo", one);
    let note = report
        .warnings
        .iter()
        .find(|w| w.contains("a.txt"))
        .unwrap();
    assert!(note.contains("a.txt; launch added it to"), "{note}");

    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let two = rig
        .validate(
            &common::with(spec, "setup", json!("touch a.txt b.txt")),
            false,
        )
        .unwrap();
    let (report, _store) = rig.launch("demo", two);
    let note = report
        .warnings
        .iter()
        .find(|w| w.contains("a.txt"))
        .unwrap();
    assert!(
        note.contains("/a.txt, /b.txt; launch added them to"),
        "{note}"
    );
}
