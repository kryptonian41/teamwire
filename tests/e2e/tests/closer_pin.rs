//! ADR 0059, through the real binary: `teamwire verify --role R --pinned-in
//! DIR` tells a closer whether its clone sits on the commit verify recorded
//! for R. The case seen: in the clone a new local branch with the member's
//! branch name, made from `main`, so every check after it read the base.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn the_pin_refuses_a_same_named_branch_at_the_base_and_accepts_the_verified_sha() {
    let step = format!(
        "{}{}{}",
        sh::commit(),
        sh::report(true),
        sh::done("wrote writer.txt")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    // A check that also passes on the base, as the one seen did.
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "writer", "title": "Writer", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "write it", "check": "true", "owns": ["writer.txt"]}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] writer finished (done)",
        Duration::from_secs(30),
    );
    let worktree = w.repo().join(".worktrees/writer");
    let writer_sha = w.git(&worktree, &["rev-parse", "HEAD"]);
    let base_sha = w.git(w.repo(), &["rev-parse", "main"]);
    assert_ne!(writer_sha, base_sha);

    let clone = w.work().join("closer-clone");
    w.git(
        w.work(),
        &[
            "clone",
            "--quiet",
            &w.repo().display().to_string(),
            "closer-clone",
        ],
    );
    let clone_arg = clone.display().to_string();
    let pin = || {
        w.teamwire(
            &[
                "verify",
                "demo",
                "--role",
                "writer",
                "--pinned-in",
                &clone_arg,
            ],
            None,
        )
    };

    // Before any verify, no commit is known to check.
    let run = pin();
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("writer has no verify record"),
        "{}",
        run.all()
    );

    let verify = w.teamwire(&["verify", "demo", "--role", "writer"], None);
    assert_eq!(verify.code, 0, "{}", verify.all());
    assert!(
        verify.out.contains(&format!("head {writer_sha}")),
        "{}",
        verify.all()
    );

    // The mistake that was seen: a new local branch of the same name, made
    // from the clone's main, with its failure masked.
    assert!(w.sh_passes(&clone, "git checkout -q -b writer || true"));
    assert_eq!(w.git(&clone, &["rev-parse", "HEAD"]), base_sha);
    let run = pin();
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err
            .contains(&format!("is at {base_sha}, not {writer_sha}")),
        "{}",
        run.all()
    );
    assert!(run.err.contains("NOT PINNED"), "{}", run.all());
    // There, the declared check passes and proves nothing: the artifact is
    // not in the tree.
    assert!(w.sh_passes(&clone, "true"));
    assert!(!w.sh_passes(&clone, "test -f writer.txt"));

    // The commit by its sha, detached, as the refusal says.
    w.git(&clone, &["fetch", "--quiet", "origin"]);
    w.git(&clone, &["checkout", "--quiet", "--detach", &writer_sha]);
    let run = pin();
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out
            .contains(&format!("pinned: writer at {writer_sha} (verify PASS)")),
        "{}",
        run.all()
    );
    assert!(w.sh_passes(&clone, "test -f writer.txt"));
    // A folder inside the right clone is not the clone: git would read the
    // clone's HEAD there, but checks run in it see none of its files.
    let child = clone.join("empty-child");
    std::fs::create_dir_all(&child).unwrap();
    let child_arg = child.display().to_string();
    let run = w.teamwire(
        &[
            "verify",
            "demo",
            "--role",
            "writer",
            "--pinned-in",
            &child_arg,
        ],
        None,
    );
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("is not the top of its own checkout"),
        "{}",
        run.all()
    );
    assert!(!w.sh_passes(&child, "test -f writer.txt"));
    std::fs::remove_dir(&child).unwrap();

    // A commit after the verify makes the record out of date.
    w.git(
        &worktree,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "late",
        ],
    );
    let run = pin();
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("but its newest verify ran at"),
        "{}",
        run.all()
    );

    // With its checkout gone, or swapped for an empty folder inside the
    // repository (where git would read the repository's own HEAD), its
    // current commit cannot be proven, and nothing is pinned.
    std::fs::remove_dir_all(&worktree).unwrap();
    for setup in ["missing", "empty"] {
        if setup == "empty" {
            std::fs::create_dir_all(&worktree).unwrap();
        }
        let run = pin();
        assert_eq!(run.code, 1, "{setup}: {}", run.all());
        assert!(
            run.err.contains("cannot verify writer's current commit"),
            "{setup}: {}",
            run.all()
        );
        assert!(!run.out.contains("pinned:"), "{setup}: {}", run.all());
    }
}

#[test]
fn a_member_verify_recorded_with_no_commit_has_nothing_to_pin() {
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(format!(
            "{}{}",
            sh::report(false),
            sh::done("looked")
        ))]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "scout", "title": "Scout", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] scout finished (done)",
        Duration::from_secs(30),
    );
    // A pane member with no check is SKIPPED, which verify counts as not
    // clean (exit 2), and its record holds no commit.
    let verify = w.teamwire(&["verify", "demo", "--role", "scout"], None);
    assert!(verify.out.contains("SKIPPED"), "{}", verify.all());
    let clone = w.work().display().to_string();
    let run = w.teamwire(
        &["verify", "demo", "--role", "scout", "--pinned-in", &clone],
        None,
    );
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(run.out.contains("there is nothing to pin"), "{}", run.all());
}

/// A member removed on purpose makes no more commits, so its gone
/// checkout is not a doubt: the pin holds on its verified commit.
#[test]
fn a_removed_member_still_pins_to_its_verified_commit() {
    let step = format!(
        "{}{}{}",
        sh::commit(),
        sh::report(true),
        sh::done("wrote writer.txt")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "writer", "title": "Writer", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "write it", "check": "test -f writer.txt",
         "owns": ["writer.txt"]}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] writer finished (done)",
        Duration::from_secs(30),
    );
    let sha = w.git(&w.repo().join(".worktrees/writer"), &["rev-parse", "HEAD"]);
    let verify = w.teamwire(&["verify", "demo", "--role", "writer"], None);
    assert_eq!(verify.code, 0, "{}", verify.all());
    let removed = w.teamwire(&["close", "demo", "--role", "writer"], None);
    assert_eq!(removed.code, 0, "{}", removed.all());
    assert!(!w.repo().join(".worktrees/writer").exists());

    w.git(
        w.work(),
        &[
            "clone",
            "--quiet",
            &w.repo().display().to_string(),
            "closer-clone",
        ],
    );
    let clone = w.work().join("closer-clone");
    w.git(&clone, &["checkout", "--quiet", "--detach", &sha]);
    let run = w.teamwire(
        &[
            "verify",
            "demo",
            "--role",
            "writer",
            "--pinned-in",
            &clone.display().to_string(),
        ],
        None,
    );
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out.contains(&format!("pinned: writer at {sha}")),
        "{}",
        run.all()
    );
}

/// A writer that committed once and finished, verified, with `check`.
fn verified_writer(check: &str) -> (World, String) {
    let step = format!(
        "{}{}{}",
        sh::commit(),
        sh::report(true),
        sh::done("wrote writer.txt")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "writer", "title": "Writer", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "write it", "check": check, "owns": ["writer.txt"]}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] writer finished (done)",
        Duration::from_secs(30),
    );
    let sha = w.git(&w.repo().join(".worktrees/writer"), &["rev-parse", "HEAD"]);
    let verify = w.teamwire(&["verify", "demo", "--role", "writer"], None);
    assert_eq!(verify.code, 0, "{}", verify.all());
    (w, sha)
}

fn pin_in(w: &World, role: &str, dir: &std::path::Path) -> teamwire_e2e::Run {
    let dir = dir.display().to_string();
    w.teamwire(
        &["verify", "demo", "--role", role, "--pinned-in", &dir],
        None,
    )
}

/// A fresh clone of the repository at `name`, checked out at `sha`.
fn clone_at(w: &World, name: &str, sha: &str, detach: bool) -> std::path::PathBuf {
    w.git(
        w.work(),
        &["clone", "--quiet", &w.repo().display().to_string(), name],
    );
    let clone = w.work().join(name);
    if detach {
        w.git(&clone, &["checkout", "--quiet", "--detach", sha]);
    } else {
        w.git(&clone, &["checkout", "--quiet", "-B", "closer-branch", sha]);
    }
    clone
}

/// The closer's reverts, resets and cleans belong in a scratch clone on the
/// verified commit, detached. The member's own checkout, the repository it
/// shares a git directory with, and a clone on a branch are refused, even
/// at the right commit.
#[test]
fn the_pin_refuses_the_members_checkout_the_shared_repository_and_an_attached_clone() {
    let (w, sha) = verified_writer("true");
    let worktree = w.repo().join(".worktrees/writer");
    for (dir, why) in [
        (
            worktree.clone(),
            "shares its git directory with writer's checkout",
        ),
        (
            w.repo().to_path_buf(),
            "shares its git directory with writer's checkout",
        ),
    ] {
        let run = pin_in(&w, "writer", &dir);
        assert_eq!(run.code, 1, "{}: {}", dir.display(), run.all());
        assert!(run.err.contains(why), "{}: {}", dir.display(), run.all());
    }
    let attached = clone_at(&w, "attached-clone", &sha, false);
    assert_eq!(w.git(&attached, &["rev-parse", "HEAD"]), sha);
    let run = pin_in(&w, "writer", &attached);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("is on branch closer-branch, not detached"),
        "{}",
        run.all()
    );
    let detached = clone_at(&w, "detached-clone", &sha, true);
    let run = pin_in(&w, "writer", &detached);
    assert_eq!(run.code, 0, "{}", run.all());
}

/// Removing a member does not make an older verify record current: the
/// branch the removal kept is the member's last commit, and the pin
/// refuses when that branch is past the record.
#[test]
fn a_removed_member_whose_branch_moved_past_its_verify_does_not_pin() {
    let (w, verified) = verified_writer("true");
    let worktree = w.repo().join(".worktrees/writer");
    w.git(
        &worktree,
        &["commit", "--quiet", "--allow-empty", "-m", "after verify"],
    );
    let last = w.git(&worktree, &["rev-parse", "HEAD"]);
    let removed = w.teamwire(&["close", "demo", "--role", "writer", "--unverified"], None);
    assert_eq!(removed.code, 0, "{}", removed.all());
    let clone = clone_at(&w, "closer-clone", &verified, true);
    let run = pin_in(&w, "writer", &clone);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains(&format!("is at {last}")) && run.err.contains("newest verify ran at"),
        "{}",
        run.all()
    );
    assert!(!run.out.contains("pinned:"), "{}", run.all());
}

/// A pane member with a check has a commit to check once verify runs, so
/// before that the pin refuses; a pane member verified outside git has a
/// record with no commit, and nothing to pin.
#[test]
fn a_pane_member_pins_by_its_record_inside_git_and_has_nothing_to_pin_outside() {
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(format!(
            "{}{}",
            sh::report(false),
            sh::done("looked")
        ))]],
        ..ClaudeScript::default()
    });
    let outside = w.work().join("notes");
    std::fs::create_dir_all(&outside).unwrap();
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "scout", "title": "Scout", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look", "check": "true"},
        {"role": "notes", "title": "Notes", "kind": "investigator", "mode": "pane",
         "cwd": outside, "task": "look", "check": "true"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] notes finished (done)",
        Duration::from_secs(30),
    );
    let missing = w.work().join("no-such-clone");
    let run = pin_in(&w, "scout", &missing);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("scout has no verify record"),
        "{}",
        run.all()
    );

    let verify = w.teamwire(&["verify", "demo", "--role", "notes"], None);
    assert_eq!(verify.code, 0, "{}", verify.all());
    let run = pin_in(&w, "notes", &missing);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(run.out.contains("there is nothing to pin"), "{}", run.all());
}
