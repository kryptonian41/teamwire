//! Making and taking back a member's git worktree (LCH-8, LCH-24). git is
//! always run through `Platform::run`, and the host's own worktree command
//! is never used (INV-23).

use std::path::{Path, PathBuf};

use teamwire_core::Platform;

use crate::WorktreePlan;
use crate::git::{failure_text, run, stdout_of};

/// What launch made, so a failure can take exactly that back.
#[derive(Debug, Clone)]
pub(crate) struct Made {
    pub(crate) repo: PathBuf,
    pub(crate) path: PathBuf,
    /// Set when launch created the branch (and so may delete it again).
    pub(crate) new_branch: Option<String>,
}

/// The commit the member starts from: the branch's tip when the branch
/// exists (the base is ignored then), else the base, else `HEAD`.
pub(crate) fn start_commit(platform: &dyn Platform, w: &WorktreePlan) -> Result<String, String> {
    let spec = if w.branch_exists {
        format!("refs/heads/{}^{{commit}}", w.branch)
    } else {
        format!("{}^{{commit}}", w.base.as_deref().unwrap_or("HEAD"))
    };
    let sha = stdout_of(platform, &w.repo, &["rev-parse", "--verify", &spec])?;
    if sha.is_empty() {
        return Err(format!("git could not name the start commit {spec}"));
    }
    Ok(sha)
}

/// `git worktree add`, at the recorded start commit.
pub(crate) fn add(platform: &dyn Platform, w: &WorktreePlan, sha: &str) -> Result<Made, String> {
    if let Some(parent) = w.path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    let path = w.path.display().to_string();
    let args: Vec<&str> = if w.branch_exists {
        vec!["worktree", "add", &path, &w.branch]
    } else {
        vec!["worktree", "add", "-b", &w.branch, &path, sha]
    };
    let out = run(platform, &w.repo, &args)?;
    if !out.success() {
        return Err(format!("git worktree add failed: {}", failure_text(&out)));
    }
    Ok(Made {
        repo: w.repo.clone(),
        path: w.path.clone(),
        new_branch: (!w.branch_exists).then(|| w.branch.clone()),
    })
}

/// Takes the worktree back without force, so uncommitted work is never
/// thrown away. Returns what happened, in words.
pub(crate) fn remove(platform: &dyn Platform, made: &Made) -> String {
    let path = made.path.display().to_string();
    let mut note = match run(platform, &made.repo, &["worktree", "remove", &path]) {
        Ok(out) if out.success() => format!("worktree {path} removed"),
        Ok(out) => format!("could not remove worktree {path}: {}", failure_text(&out)),
        Err(e) => format!("could not remove worktree {path}: {e}"),
    };
    if let Some(branch) = &made.new_branch {
        // `-d` refuses a branch with commits that are not merged, which keeps
        // any work a member managed to commit.
        match run(platform, &made.repo, &["branch", "-d", branch]) {
            Ok(out) if out.success() => note = format!("{note}; branch {branch} deleted"),
            _ => note = format!("{note}; branch {branch} left in place"),
        }
    }
    note
}

/// The repository's `info/exclude` for this worktree, found by git.
fn exclude_path(platform: &dyn Platform, worktree: &Path) -> Result<PathBuf, String> {
    let text = stdout_of(
        platform,
        worktree,
        &["rev-parse", "--git-path", "info/exclude"],
    )?;
    let path = PathBuf::from(text);
    Ok(if path.is_absolute() {
        path
    } else {
        worktree.join(path)
    })
}

/// Adds `TASK.md` to the repository's `info/exclude` (LCH-24), once.
pub(crate) fn exclude_task_md(platform: &dyn Platform, worktree: &Path) -> Result<(), String> {
    let path = exclude_path(platform, worktree)?;
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if text
        .lines()
        .any(|l| matches!(l.trim(), "TASK.md" | "/TASK.md"))
    {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("TASK.md\n");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    platform
        .write_atomic(&path, text.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}
