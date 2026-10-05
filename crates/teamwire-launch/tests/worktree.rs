//! Worktrees: made with git directly, at a recorded commit, with the brief
//! beside the code but never committed.

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, git, member_row, one_worktree_team, role_events};
use teamwire_core::MemberState;
use teamwire_launch::LaunchOutcome;

#[test]
fn worktree_made_with_git_and_base_sha_recorded() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let plan = rig.validate(&spec, false).unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );

    let path = repo.join(".worktrees").join("api");
    assert!(path.join("README.md").is_file(), "the checkout exists");
    assert!(git(&repo, &["worktree", "list", "--porcelain"]).contains(&path.display().to_string()));
    assert_eq!(git(&path, &["rev-parse", "--abbrev-ref", "HEAD"]), "api");
    let row = member_row(&store, "api").unwrap();
    assert_eq!(row.base_sha.as_deref(), Some(head.as_str()));
    assert_eq!(row.branch.as_deref(), Some("api"));
    assert_eq!(row.cwd, path);
    assert_eq!(row.state, MemberState::Started);
    assert!(row.place.is_some());
    assert!(row.session_id.is_some());
    // The host was asked to start an agent in the worktree, and nothing else
    // about worktrees (the fake host has no worktree call at all).
    let place = row.place.unwrap();
    assert_eq!(rig.host.cwd_of(&place), Some(path));
}

#[test]
fn a_new_branch_starts_at_the_named_base_and_an_existing_branch_is_reused() {
    let rig = Rig::new();
    let (mut spec, repo) = one_worktree_team(&rig, "demo");
    let first = git(&repo, &["rev-parse", "HEAD"]);
    common::write(&repo.join("more.txt"), "more\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "second"]);
    let child = spec
        .get_mut("children")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .first_mut()
        .unwrap();
    child["base"] = serde_json::json!(first);
    child["branch"] = serde_json::json!("feature/x");
    let plan = rig.validate(&spec, false).unwrap();
    let (_, store) = rig.launch("demo", plan);
    let row = member_row(&store, "api").unwrap();
    assert_eq!(row.base_sha.as_deref(), Some(first.as_str()));
    assert!(
        repo.join(".worktrees/feature-x").is_dir(),
        "a slash becomes a dash in the directory"
    );

    // A second launch of the same branch warns that the base is ignored.
    git(&repo, &["worktree", "remove", ".worktrees/feature-x"]);
    let plan = rig
        .validate(
            &common::team_spec("other", &[common::worktree_member("b", "Bee", &repo)]),
            false,
        )
        .unwrap();
    assert!(plan.warnings.iter().all(|w| !w.contains("already exists")));
    let again = common::with(
        common::team_spec(
            "third",
            &[common::with(
                common::worktree_member("c", "Cee", &repo),
                "branch",
                serde_json::json!("feature/x"),
            )],
        ),
        "title",
        serde_json::json!("Third"),
    );
    let plan = rig.validate(&again, false).unwrap();
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("feature/x already exists"))
    );
}

#[test]
fn task_md_written_and_excluded_from_git() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let path = repo.join(".worktrees").join("api");
    assert_eq!(
        std::fs::read_to_string(path.join("TASK.md")).unwrap(),
        "build it"
    );
    // The member runs `git add -A`: the brief must not be staged.
    common::write(&path.join("work.rs"), "fn main() {}\n");
    git(&path, &["add", "-A"]);
    let staged = git(&path, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged, "work.rs");
    assert_eq!(git(&path, &["status", "--porcelain"]), "A  work.rs");
    // A copy is kept with the team, outside the worktree.
    let kept = rig.home.path().join("teams/demo/briefs/api.md");
    assert_eq!(std::fs::read_to_string(kept).unwrap(), "build it");
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert_eq!(exclude.lines().filter(|l| *l == "TASK.md").count(), 1);
}

#[test]
fn events_carry_the_launch_id_the_requested_count_steps_and_session() {
    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let plan = rig.validate(&spec, false).unwrap();
    let (_, store) = rig.launch("demo", plan);
    let events = role_events(&store, "api");
    let kinds: Vec<&str> = events.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, ["member.creating", "member.started"]);
    let launch = common::jget(&events.first().unwrap().1, "/launch")
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(launch.len(), 26);
    for (_, data) in &events {
        assert_eq!(data["launch"], launch.as_str());
        assert_eq!(data["requested"], 1);
    }
    let started = &events.last().unwrap().1;
    assert!(common::jget(started, "/steps/worktree").is_number());
    assert!(common::jget(started, "/session_id").is_string());
}

#[test]
fn tracked_task_md_is_refused() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    common::write(&repo.join("TASK.md"), "the project's own file\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "track TASK.md"]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(common::codes(&errors), ["task_md_tracked"]);
    assert!(errors.first().unwrap().message.contains("TASK.md"));
    assert_eq!(
        std::fs::read_to_string(repo.join("TASK.md")).unwrap(),
        "the project's own file\n",
        "the tracked file is untouched"
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a test seeds the repository's exclude file with bytes that are not UTF-8"
)]
fn non_utf8_exclude_is_kept() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    std::fs::write(&exclude, b"caf\xe9\n").unwrap();
    let plan = rig.validate(&spec, false).unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let bytes = std::fs::read(&exclude).unwrap();
    assert!(bytes.starts_with(b"caf\xe9\n"), "{bytes:?}");
    assert!(bytes.ends_with(b"TASK.md\n"), "{bytes:?}");
}
