//! `teamwire verify --role R --pinned-in DIR`: whether a closer's clone sits
//! on the commit verify recorded for R (ADR 0059). It runs no check.
//!
//! A closer once ran `git checkout -b <team>/<role> || true` in its clone.
//! That made a new local branch at the clone's `main`, not the member's
//! commit, and every check after it inspected the wrong revision. A branch
//! name is not a commit, and a command whose failure is masked is not
//! evidence. So the commit to check is the one the team's store names: the
//! `head` of R's newest verify record, which `teamwire verify` prints. The
//! pin compares three commits by their full sha and nothing else:
//!
//! - the newest verify record's head, the commit the closer must check;
//! - R's worktree HEAD now, so a record older than the member's last
//!   commit is reported as out of date, not taken as evidence. A HEAD that
//!   cannot be proven (no checkout, not a checkout of its own) is refused
//!   too. A removed worktree member's last commit is the tip of the branch
//!   its removal kept, so that tip is compared instead: removing a member
//!   does not make an older record current;
//! - the clone's HEAD, which must be detached, in a clone that shares no
//!   git directory with the member's checkout: the closer reverts, resets
//!   and cleans there, which must never touch the member's own work.
//!
//! A member with no commit to check (a pane member with no check, which
//! verify skips, or one recorded with no commit, `none` outside git) has
//! nothing to pin, and the pin says so. Any other member with no verify
//! record is refused: its commit is unknown.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use teamwire_core::{MemberState, Role};
use teamwire_store::Ctx;

use crate::VerifyError;
use crate::git::Git;

/// What the pin found for one member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub role: Role,
    /// The head of the newest verify record: the commit to check.
    pub expected: Option<String>,
    /// The verdict of that record.
    pub verdict: Option<String>,
    /// The member's worktree HEAD now.
    pub worktree_head: Option<String>,
    /// The clone's HEAD.
    pub clone_head: Option<String>,
    pub clone: PathBuf,
    /// Why the clone's evidence is not evidence for the expected commit.
    /// Empty when the clone sits on it, or when there is nothing to pin.
    pub problems: Vec<String>,
    /// Set when verify recorded no commit for the member.
    pub nothing_to_pin: Option<String>,
}

impl Pin {
    /// True when evidence from the clone may be taken: it sits on the
    /// commit verify recorded, and that record is the member's newest.
    #[must_use]
    pub fn holds(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The pin of `role` against the clone at `clone`. Reads the store and git;
/// changes nothing.
pub fn pin(ctx: &Ctx<'_>, role: &Role, clone: &Path) -> Result<Pin, VerifyError> {
    let member = ctx
        .store
        .read(|tx| tx.member(role))?
        .ok_or_else(|| VerifyError::UnknownRole(role.clone()))?;
    let newest = ctx
        .store
        .read(|tx| tx.verify_records(role))?
        .items
        .into_iter()
        .last();
    let mut out = Pin {
        role: role.clone(),
        expected: None,
        verdict: newest.as_ref().map(|r| r.record.verdict.clone()),
        worktree_head: None,
        clone_head: None,
        clone: clone.to_path_buf(),
        problems: Vec::new(),
        nothing_to_pin: None,
    };
    let Some(newest) = newest else {
        // A pane member with no check is skipped by verify, which records
        // no commit for it. A pane member with a check gets a record with
        // its commit once verify runs, so until then its commit is unknown.
        if member.mode != teamwire_core::Mode::Worktree && member.check.is_none() {
            out.nothing_to_pin = Some(format!(
                "{role} works in a pane with no check and no verify record: there is nothing to \
                 pin"
            ));
            return Ok(out);
        }
        out.problems.push(format!(
            "{role} has no verify record, so no commit is known to check: run `teamwire verify \
             <team> --role {role}` first"
        ));
        return Ok(out);
    };
    let expected = newest.record.head_sha.trim().to_owned();
    // Verify writes `none` for a member that works outside git.
    if expected.is_empty() || expected == "none" {
        out.nothing_to_pin = Some(format!(
            "verify recorded {role} as {} with no commit: there is nothing to pin",
            newest.record.verdict
        ));
        return Ok(out);
    }
    out.expected = Some(expected.clone());
    // The member's current commit must be proven: one that cannot be read
    // is not taken to match.
    {
        match current_head(ctx, &member) {
            Ok(now) => {
                if now != expected && member.state == MemberState::Removed {
                    out.problems.push(format!(
                        "{role} was removed, and its kept branch is at {now}, but its newest \
                         verify ran at {expected}: verify skips a removed member, so nothing \
                         judged {now}; report the evidence for it as missing"
                    ));
                } else if now != expected {
                    out.problems.push(format!(
                        "{role}'s worktree is now at {now}, but its newest verify ran at \
                         {expected}: run `teamwire verify <team> --role {role}` again before you \
                         take evidence"
                    ));
                }
                out.worktree_head = Some(now);
            }
            Err(why) => out.problems.push(format!(
                "cannot verify {role}'s current commit: {why}; the record at {expected} may be \
                 out of date, so it is not taken as evidence"
            )),
        }
    }
    if !clone.is_dir() {
        out.problems.push(format!(
            "{} does not exist: clone the repository there first",
            clone.display()
        ));
        return Ok(out);
    }
    // Only the commit is proven here, never a file or a check: the closer
    // still runs each check in the clone and reports its exit code.
    match root_head(ctx, clone) {
        Ok(head) => {
            out.problems
                .extend(clone_problems(ctx, &member, clone, &expected));
            if head != expected {
                out.problems.push(format!(
                    "the clone at {dir} is at {head}, not {expected}, the commit verify recorded \
                     for {role}; a branch of the same name in the clone is not that commit. Check \
                     the commit out by its sha: git -C {dir} fetch --quiet origin && git -C {dir} \
                     checkout --quiet --detach {expected}",
                    dir = clone.display()
                ));
            }
            out.clone_head = Some(head);
        }
        Err(why) => out.problems.push(format!(
            "the clone cannot be pinned: {why}; name the clone's own root folder"
        )),
    }
    Ok(out)
}

/// Why the clone may not hold the closer's evidence, beyond its commit: it
/// is on a branch, so a later fetch or checkout can move it without the sha
/// changing in the report; or it shares its git directory with the
/// member's checkout, so a revert, `git reset --hard` or `git clean` there
/// changes the member's own repository.
fn clone_problems(
    ctx: &Ctx<'_>,
    member: &teamwire_core::Member,
    clone: &Path,
    expected: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    let git = Git::new(ctx.platform, clone);
    if let Ok(branch) = git.stdout(&["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        out.push(format!(
            "the clone at {dir} is on branch {branch}, not detached: check the commit out by its \
             sha with git -C {dir} checkout --quiet --detach {expected}",
            dir = clone.display(),
            branch = branch.trim(),
        ));
    }
    let member_dir = if member.cwd.is_dir() {
        Some(member.cwd.as_path())
    } else {
        member.repo.as_deref()
    };
    let shared = member_dir
        .and_then(|d| common_dir(ctx, d))
        .zip(common_dir(ctx, clone))
        .is_some_and(|(a, b)| a == b);
    if shared {
        out.push(format!(
            "the clone at {} shares its git directory with {}'s checkout: a closer's reverts, \
             resets and cleans there would change the member's own repository. Use a separate \
             clone made with git clone",
            clone.display(),
            member.role
        ));
    }
    out
}

/// The git directory that `dir`'s checkout shares with its other worktrees,
/// with links resolved.
fn common_dir(ctx: &Ctx<'_>, dir: &Path) -> Option<PathBuf> {
    let out = Git::new(ctx.platform, dir)
        .stdout(&["rev-parse", "--git-common-dir"])
        .ok()?;
    std::fs::canonicalize(dir.join(out.trim())).ok()
}

/// The commit the member's checkout is on now. A worktree member's folder
/// must be the root of its own checkout (`root_head`). A removed worktree
/// member has no folder: its last commit is the tip of the branch its
/// removal kept. A pane member works in a folder of a repository, so its
/// HEAD is read where it works. Every failure is returned with its reason,
/// never taken for a match.
fn current_head(ctx: &Ctx<'_>, member: &teamwire_core::Member) -> Result<String, String> {
    if member.state == MemberState::Removed && member.mode == teamwire_core::Mode::Worktree {
        return kept_branch_tip(ctx, member);
    }
    let cwd = &member.cwd;
    if !cwd.is_dir() {
        return Err(format!("its checkout {} does not exist", cwd.display()));
    }
    if member.mode == teamwire_core::Mode::Worktree {
        return root_head(ctx, cwd);
    }
    Git::new(ctx.platform, cwd)
        .head()
        .map_err(|e| format!("the HEAD of {} cannot be read ({e})", cwd.display()))
}

/// The HEAD of the checkout whose root is `dir`. The member's worktree and
/// the closer's clone are both read this way. A folder inside a checkout is
/// refused: git would read the outer checkout's HEAD there, and checks run
/// in that folder would see none of its files. Paths are compared after
/// resolving links, so two names of one folder match.
fn root_head(ctx: &Ctx<'_>, dir: &Path) -> Result<String, String> {
    if !dir.is_dir() {
        return Err(format!("{} does not exist", dir.display()));
    }
    let git = Git::new(ctx.platform, dir);
    let top = git
        .stdout(&["rev-parse", "--show-toplevel"])
        .map_err(|e| format!("{} is not a git checkout ({e})", dir.display()))?;
    let same = std::fs::canonicalize(top.trim())
        .ok()
        .zip(std::fs::canonicalize(dir).ok())
        .is_some_and(|(a, b)| a == b);
    if !same {
        return Err(format!(
            "{} is not the top of its own checkout (git reads {} there)",
            dir.display(),
            top.trim()
        ));
    }
    git.head()
        .map_err(|e| format!("the HEAD of {} cannot be read ({e})", dir.display()))
}

/// The tip of the branch a removed worktree member's removal kept.
fn kept_branch_tip(ctx: &Ctx<'_>, member: &teamwire_core::Member) -> Result<String, String> {
    let (Some(repo), Some(branch)) = (member.repo.as_deref(), member.branch.as_deref()) else {
        return Err(format!(
            "{} was removed and its row names no repository and branch to read",
            member.role
        ));
    };
    let reference = format!("refs/heads/{branch}^{{commit}}");
    Git::new(ctx.platform, repo)
        .stdout(&["rev-parse", "--verify", "--quiet", &reference])
        .map(|sha| sha.trim().to_owned())
        .map_err(|e| {
            format!(
                "{} was removed, and its kept branch {branch} cannot be read in {} ({e})",
                member.role,
                repo.display()
            )
        })
}
