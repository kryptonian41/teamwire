//! The git side of checking a worktree member: branch, base, slug, ignore rule.

use std::path::Path;

use teamwire_core::MemberSpec;

use crate::WorktreePlan;
use crate::git;
use crate::plan::{Bad, Cx, branch_ok};

pub(crate) fn worktree_plan(
    cx: &mut Cx<'_>,
    child: &MemberSpec,
    repo: &Path,
    bad: &mut Bad<'_>,
    warnings: &mut Vec<String>,
) -> Option<WorktreePlan> {
    let role = child.role.as_str();
    let platform = cx.env.platform;
    let branch = child.branch.clone().unwrap_or_else(|| role.to_owned());
    if !branch_ok(&branch) {
        bad(
            "branch",
            "bad_branch",
            format!("branch {branch:?} is not a usable branch name"),
        );
        return None;
    }
    let slug = branch.replace('/', "-");
    if let Some(other) = cx
        .slugs
        .insert((repo.to_path_buf(), slug.clone()), role.to_owned())
    {
        bad(
            "branch",
            "slug_collision",
            format!("roles {other} and {role} both map to the directory .worktrees/{slug}"),
        );
        return None;
    }
    let path = repo.join(".worktrees").join(&slug);
    if path.exists() {
        bad(
            "branch",
            "path_exists",
            format!("{} already exists", path.display()),
        );
        return None;
    }
    match git::run(platform, repo, &["rev-parse", "--git-dir"]) {
        Ok(out) if out.success() => {}
        Ok(out) => {
            bad(
                "repo",
                "not_a_repo",
                format!(
                    "{} is not a git repository: {}",
                    repo.display(),
                    git::failure_text(&out)
                ),
            );
            return None;
        }
        Err(e) => {
            bad("repo", "git_failed", e);
            return None;
        }
    }
    let head = format!("refs/heads/{branch}");
    let branch_exists = git::run(platform, repo, &["rev-parse", "--verify", "--quiet", &head])
        .is_ok_and(|o| o.success());
    if branch_exists {
        let ignored = child
            .base
            .as_ref()
            .map_or(String::new(), |b| format!("; base {b} is ignored"));
        warnings.push(format!(
            "role {role}: branch {branch} already exists and is reused{ignored}"
        ));
    } else if let Some(base) = &child.base {
        let spec = format!("{base}^{{commit}}");
        if !git::run(platform, repo, &["rev-parse", "--verify", "--quiet", &spec])
            .is_ok_and(|o| o.success())
        {
            bad(
                "base",
                "bad_base",
                format!("base {base} is not a commit in {}", repo.display()),
            );
            return None;
        }
    }
    let ignore_path = format!(".worktrees/{slug}/x");
    if git::run(platform, repo, &["check-ignore", "-q", &ignore_path])
        .is_ok_and(|o| o.exit_code == 1)
    {
        warnings.push(format!(
            "role {role}: .worktrees is not ignored by {}; add it to .gitignore or `git add -A` will commit the checkout",
            repo.display()
        ));
    }
    Some(WorktreePlan {
        repo: repo.to_path_buf(),
        branch,
        path,
        base: child.base.clone(),
        branch_exists,
    })
}
