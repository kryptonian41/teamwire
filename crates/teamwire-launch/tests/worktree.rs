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
    assert_eq!(exclude.lines().filter(|l| *l == "/TASK.md").count(), 1);
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
    assert!(bytes.ends_with(b"\n/TASK.md\n"), "{bytes:?}");
}

#[test]
fn a_worktree_member_is_refused_when_worktrees_is_not_ignored() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    common::write(&repo.join(".gitignore"), "target/\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "stop ignoring .worktrees"]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(common::codes(&errors), ["worktree_not_ignored"]);
}

#[test]
fn a_pane_only_team_does_not_need_worktrees_ignored() {
    let rig = Rig::new();
    let repo = rig.repo("plain");
    common::write(&repo.join(".gitignore"), "target/\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "stop ignoring .worktrees"]);
    let spec = common::team_spec("demo", &[common::pane_member("a", "Alpha", &repo)]);
    assert!(rig.validate(&spec, false).is_ok());
}

#[test]
fn the_contract_is_written_once() {
    let rig = Rig::new();
    let (spec, _repo) = one_worktree_team(&rig, "demo");
    let plan = rig.validate(&spec, false).unwrap();
    let _ = rig.launch("demo", plan);
    let contract = rig.home.path().join("teams/demo/contract/api.md");
    let writes = rig.platform.inner.written();
    assert_eq!(
        writes.iter().filter(|p| **p == contract).count(),
        1,
        "{writes:?}"
    );
}

#[test]
fn only_the_brief_at_the_root_is_hidden_from_git() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let plan = rig.validate(&spec, false).unwrap();
    let _ = rig.launch("demo", plan);
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    common::write(&repo.join("docs/TASK.md"), "the user's own notes\n");
    assert_eq!(git(&repo, &["status", "--porcelain"]), "?? docs/");
}

/// The lock file launch and close take for `repo`'s `info/exclude`.
#[cfg(test)]
fn exclude_lock(rig: &Rig, repo: &std::path::Path) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let info = std::fs::canonicalize(repo.join(".git/info")).unwrap();
    let key = info.join("exclude");
    let digest = Sha256::digest(key.to_string_lossy().as_bytes());
    rig.home
        .path()
        .join("locks")
        .join(format!("exclude-{}.lock", hex::encode(digest)))
}

/// Holds `repo`'s exclude lock through an open file of the test's own, as
/// another teamwire process would. Dropping the file releases the lock.
#[cfg(test)]
fn hold_exclude_lock(rig: &Rig, repo: &std::path::Path) -> std::fs::File {
    let path = exclude_lock(rig, repo);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    if !path.exists() {
        common::write(&path, "");
    }
    let file = std::fs::File::open(&path).unwrap();
    file.try_lock().unwrap();
    file
}

/// True once `path` exists. Waits up to 60 s, the suite's budget, and gives
/// up early when the launch has finished without making it.
fn appears<T>(path: &std::path::Path, launch: &std::thread::ScopedJoinHandle<'_, T>) -> bool {
    for _ in 0..1200 {
        if path.exists() || launch.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    path.exists()
}

/// A team of one worktree member on `repo` whose setup creates `files`.
fn team_making(team: &str, role: &str, repo: &std::path::Path, files: &str) -> serde_json::Value {
    let member = common::with(
        common::worktree_member(role, role, repo),
        "setup",
        serde_json::json!(format!("touch {files}")),
    );
    common::team_spec(team, &[member])
}

#[test]
fn two_launches_on_one_repository_at_once_keep_both_sets_of_lines() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let start = std::sync::Barrier::new(2);
    std::thread::scope(|s| {
        for (team, role, files) in [("one", "a", "a1 a2 a3"), ("two", "b", "b1 b2 b3")] {
            let (rig, repo, start) = (&rig, &repo, &start);
            s.spawn(move || {
                let plan = rig
                    .validate(&team_making(team, role, repo, files), false)
                    .unwrap();
                start.wait();
                let (report, _) = rig.launch(team, plan);
                assert_eq!(
                    report.members.first().unwrap().outcome,
                    LaunchOutcome::Started
                );
            });
        }
    });
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    for line in ["/a1", "/a2", "/a3", "/b1", "/b2", "/b3", "/TASK.md"] {
        assert_eq!(
            exclude.lines().filter(|l| *l == line).count(),
            1,
            "{line} in {exclude}"
        );
    }
    let lock = exclude_lock(&rig, &repo);
    assert!(
        std::fs::File::open(&lock).unwrap().try_lock().is_ok(),
        "both launches released the lock"
    );
    let left: Vec<std::path::PathBuf> = std::fs::read_dir(lock.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(
        left,
        [lock],
        "the lock file stays, and no lock folder or pid file is made"
    );
}

#[test]
fn a_launch_waits_for_a_live_exclude_lock() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let exclude = repo.join(".git/info/exclude");
    common::write(&exclude, "# the user's own\n*.log\n");
    // Another teamwire process's hold: its own open file, locked.
    let held = hold_exclude_lock(&rig, &repo);
    std::thread::scope(|s| {
        let launch = s.spawn(|| rig.launch("demo", rig.validate(&spec, false).unwrap()));
        // Launch writes the brief right before it asks for the lock.
        let brief = repo.join(".worktrees/api/TASK.md");
        assert!(appears(&brief, &launch), "launch wrote the brief");
        std::thread::sleep(std::time::Duration::from_millis(500));
        let during = std::fs::read_to_string(&exclude).unwrap_or_default();
        assert!(
            !during.contains("/TASK.md"),
            "written under a held lock: {during}"
        );
        drop(held);
        let (report, _) = launch.join().unwrap();
        assert_eq!(
            report.members.first().unwrap().outcome,
            LaunchOutcome::Started
        );
    });
    let after = std::fs::read_to_string(&exclude).unwrap();
    assert_eq!(after, "# the user's own\n*.log\n/TASK.md\n");
}

#[test]
fn an_exclude_lock_whose_holder_closed_its_file_is_taken_at_once() {
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    // A holder that is gone: its file took the lock and was closed.
    drop(hold_exclude_lock(&rig, &repo));
    let (report, _) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|l| l == "/TASK.md"), "{exclude}");
    assert!(
        exclude_lock(&rig, &repo).is_file(),
        "the lock file is never deleted"
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a real process must take the lock and be killed while it holds it"
)]
fn an_exclude_lock_held_by_a_killed_process_is_taken_at_once() {
    use std::io::BufRead;
    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let lock = exclude_lock(&rig, &repo);
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    // perl's flock is the kernel lock that `File::try_lock` takes.
    let mut holder = std::process::Command::new("/usr/bin/perl")
        .args([
            "-MFcntl=:flock",
            "-e",
            r#"$| = 1; open(my $f, ">>", $ARGV[0]) or die "open: $!"; flock($f, LOCK_EX) or die "flock: $!"; print "held\n"; sleep 60"#,
        ])
        .arg(&lock)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut said = String::new();
    std::io::BufReader::new(holder.stdout.take().unwrap())
        .read_line(&mut said)
        .unwrap();
    assert_eq!(said, "held\n");
    assert!(
        std::fs::File::open(&lock).unwrap().try_lock().is_err(),
        "the child holds the lock"
    );
    holder.kill().unwrap();
    holder.wait().unwrap();
    let (report, _) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    assert_eq!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Started
    );
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|l| l == "/TASK.md"), "{exclude}");
}

#[test]
fn two_branches_that_map_to_one_directory_are_refused() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let member = |role: &str, branch: &str| {
        common::with(
            common::worktree_member(role, role, &repo),
            "branch",
            serde_json::json!(branch),
        )
    };
    let spec = common::team_spec("demo", &[member("a", "feat/x"), member("b", "feat-x")]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(common::codes(&errors), ["slug_collision"]);
    let message = &errors.first().unwrap().message;
    assert!(
        message.contains("a and b") && message.contains(".worktrees/feat-x"),
        "{message}"
    );
}

#[test]
fn a_base_that_is_not_a_commit_is_refused() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let member = common::with(
        common::worktree_member("api", "API work", &repo),
        "base",
        serde_json::json!("no-such-ref"),
    );
    let spec = common::team_spec("demo", &[member]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(common::codes(&errors), ["bad_base"]);
    assert!(errors.first().unwrap().message.contains("no-such-ref"));
}

#[test]
fn a_close_taking_its_line_back_during_setup_does_not_lose_the_new_members_line() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let exclude = repo.join(".git/info/exclude");
    // Another team's setup line, which that team's close is about to take back.
    common::write(&exclude, "/gen-x\n");
    let held = hold_exclude_lock(&rig, &repo);
    let made = repo.join(".worktrees/b/gen-x");
    let pause = std::time::Duration::from_millis(50);
    std::thread::scope(|s| {
        let launch = s.spawn(|| {
            let spec = team_making("two", "b", &repo, "gen-x");
            rig.launch("two", rig.validate(&spec, false).unwrap())
        });
        assert!(appears(&made, &launch), "setup ran");
        // Let launch get as far as it can before the close lets go.
        std::thread::sleep(pause * 10);
        // The other team's close, holding the lock, takes its line back.
        common::write(&exclude, "");
        drop(held);
        let (report, _) = launch.join().unwrap();
        assert_eq!(
            report.members.first().unwrap().outcome,
            LaunchOutcome::Started
        );
    });
    let after = std::fs::read_to_string(&exclude).unwrap();
    assert!(after.lines().any(|l| l == "/gen-x"), "{after}");
    assert_eq!(
        git(&repo.join(".worktrees/b"), &["status", "--porcelain"]),
        ""
    );
}
