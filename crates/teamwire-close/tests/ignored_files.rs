//! `git worktree remove` deletes gitignored files with no copy, and the
//! dirty check does not list them: the plan and the report say so first.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a person who edits files in a worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::Rig;
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close};
use teamwire_core::MemberKind;

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
