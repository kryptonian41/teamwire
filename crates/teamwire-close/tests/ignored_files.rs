//! `git worktree remove` deletes gitignored files with no copy, and the
//! dirty check does not list them: the plan and the report say so first.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a person who edits files in a worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, git};
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close};
use teamwire_core::{Clock, EventType, MemberKind, Role};

/// A rig whose worktree `api` holds ignored files named `names`. The pattern
/// goes in the repository's own exclude file, so the worktree stays clean.
fn rig_with_ignored(names: &[&str]) -> std::io::Result<Rig> {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    std::fs::write(rig.repo.join(".git/info/exclude"), "*.local\n")?;
    for name in names {
        std::fs::write(api.cwd.join(name), "secret\n")?;
    }
    Ok(rig)
}

fn warning(lines: impl IntoIterator<Item = (String, String)>) -> Vec<String> {
    lines
        .into_iter()
        .filter(|(_, detail)| detail.starts_with("removal deletes ignored files"))
        .map(|(role, detail)| format!("{role}: {detail}"))
        .collect()
}

#[test]
fn the_plan_and_the_report_name_the_ignored_files_the_removal_deletes() -> std::io::Result<()> {
    let mut rig = rig_with_ignored(&[".env.local"])?;
    let api = rig.member("api");
    let want = vec!["api: removal deletes ignored files: .env.local".to_owned()];

    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{plan:?}");
    let lines = warning(plan.steps.into_iter().map(|s| (s.step, s.detail)));
    assert_eq!(lines, want);

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    let lines = warning(report.steps.into_iter().map(|s| (s.step, s.detail)));
    assert_eq!(lines, want);
    assert!(!api.cwd.exists(), "the warning is not a refusal");
    Ok(())
}
#[test]
fn a_worktree_with_no_ignored_files_gets_no_warning() -> std::io::Result<()> {
    let mut rig = rig_with_ignored(&[])?;
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(warning(plan.steps.into_iter().map(|s| (s.step, s.detail))).is_empty());
    Ok(())
}
#[test]
fn more_than_ten_ignored_files_are_counted_after_the_tenth() -> std::io::Result<()> {
    let names: Vec<String> = (0..13).map(|i| format!("f{i:02}.local")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut rig = rig_with_ignored(&refs)?;
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    let lines = warning(plan.steps.into_iter().map(|s| (s.step, s.detail)));
    let shown = names
        .iter()
        .take(10)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(
        lines,
        [format!(
            "api: removal deletes ignored files: {shown}, and 3 more"
        )]
    );
    Ok(())
}
#[test]
fn the_briefs_own_copy_is_not_named() -> std::io::Result<()> {
    let mut rig = rig_with_ignored(&[])?;
    let api = rig.member("api");
    std::fs::write(rig.repo.join(".git/info/exclude"), "TASK.md\n")?;
    std::fs::write(api.cwd.join("TASK.md"), "brief\n")?;
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(warning(plan.steps.into_iter().map(|s| (s.step, s.detail))).is_empty());
    Ok(())
}

#[test]
fn build_folders_are_not_named_but_everything_else_is() -> std::io::Result<()> {
    let mut rig = rig_with_ignored(&[])?;
    let api = rig.member("api");
    std::fs::write(
        rig.repo.join(".git/info/exclude"),
        "*.local\ntarget/\nnode_modules/\n",
    )?;
    std::fs::create_dir_all(api.cwd.join("target"))?;
    std::fs::write(api.cwd.join("target/out"), "x")?;
    std::fs::create_dir_all(api.cwd.join("node_modules"))?;
    std::fs::write(api.cwd.join("node_modules/m"), "x")?;
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(warning(plan.steps.into_iter().map(|s| (s.step, s.detail))).is_empty());

    std::fs::write(api.cwd.join(".env.local"), "secret\n")?;
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert_eq!(
        warning(plan.steps.into_iter().map(|s| (s.step, s.detail))),
        ["api: removal deletes ignored files: .env.local"]
    );
    Ok(())
}

/// Records `lines` in `role`'s `member.started` data as the lines launch
/// added to the repository's exclude file.
#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn record_excluded(rig: &mut Rig, role: &str, lines: &[&str]) {
    let file = rig.repo.join(".git/info/exclude").display().to_string();
    let data = serde_json::json!({ "excluded": [{ "file": file, "lines": lines }] });
    let now = rig.clock.now();
    let role = Role::parse(role).unwrap();
    rig.store
        .write(|tx| {
            tx.record_event(now, EventType::MemberStarted, Some(&role), None, &data)
                .map(|_| ())
        })
        .unwrap();
}

/// A linked worktree of the repository that this team does not own, as
/// another team's member would have it, holding `files`.
fn other_worktree(rig: &Rig, files: &[&str]) -> std::io::Result<std::path::PathBuf> {
    let path = rig.work.path().join("wt-other");
    git(
        &rig.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            &path.display().to_string(),
        ],
    );
    for file in files {
        let at = path.join(file);
        if let Some(dir) = at.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(at, "x\n")?;
    }
    Ok(path)
}

#[test]
fn close_takes_back_launchs_lines_unless_another_worktree_needs_them() -> std::io::Result<()> {
    let mut rig = rig_with_ignored(&[])?;
    let api = rig.member("api");
    let exclude = rig.repo.join(".git/info/exclude");
    std::fs::write(&exclude, "# mine\n/TASK.md\n/gen/api.out\n/gen/shared\n")?;
    record_excluded(
        &mut rig,
        "api",
        &["/TASK.md", "/gen/api.out", "/gen/shared"],
    );
    std::fs::write(api.cwd.join("TASK.md"), "brief\n")?;
    std::fs::create_dir_all(api.cwd.join("gen"))?;
    std::fs::write(api.cwd.join("gen/api.out"), "x\n")?;
    other_worktree(&rig, &["TASK.md", "gen/shared"])?;

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&exclude)?,
        "# mine\n/TASK.md\n/gen/shared\n",
        "the other worktree still needs /TASK.md and /gen/shared"
    );
    let step = report.steps.iter().find(|s| s.step == "remove_worktrees");
    assert!(
        step.is_some_and(|s| s.detail.contains("took back /gen/api.out from")),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn the_team_that_closes_last_takes_back_task_md_another_team_added() -> std::io::Result<()> {
    // Team one launched first and recorded /TASK.md; this team found it there
    // and recorded nothing. Team one has closed, and the user keeps a TASK.md
    // of their own in the main checkout.
    let mut rig = rig_with_ignored(&[])?;
    let exclude = rig.repo.join(".git/info/exclude");
    std::fs::write(&exclude, "*.local\n/TASK.md\n")?;
    std::fs::write(rig.repo.join("TASK.md"), "the user's own notes\n")?;

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    assert_eq!(std::fs::read_to_string(&exclude)?, "*.local\n");
    assert_eq!(git(&rig.repo, &["status", "--porcelain"]), "?? TASK.md");
    Ok(())
}

#[test]
fn task_md_stays_while_another_teams_worktree_needs_it() -> std::io::Result<()> {
    let mut rig = rig_with_ignored(&[])?;
    let exclude = rig.repo.join(".git/info/exclude");
    std::fs::write(&exclude, "/TASK.md\n")?;
    record_excluded(&mut rig, "api", &["/TASK.md"]);
    other_worktree(&rig, &["TASK.md"])?;

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    assert_eq!(std::fs::read_to_string(&exclude)?, "/TASK.md\n");
    Ok(())
}
