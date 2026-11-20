//! `teamwire verify --role R --pinned-in DIR`: whether a closer's clone sits
//! on the commit verify recorded for R (ADR 0057). It runs no check.
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
//!   commit is reported as out of date, not taken as evidence;
//! - the clone's HEAD.
//!
//! A member with no commit to check (a pane member verify skipped, or one
//! recorded with no commit) has nothing to pin, and the pin says so. A
//! worktree member with no verify record is refused: its commit is unknown.

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
        // A pane member keeps no branch of its own, and verify records none
        // of its runs when it has no check: no commit was made to check.
        if member.mode != teamwire_core::Mode::Worktree {
            out.nothing_to_pin = Some(format!(
                "{role} works in a pane with no verify record: there is nothing to pin"
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
    if expected.is_empty() {
        out.nothing_to_pin = Some(format!(
            "verify recorded {role} as {} with no commit: there is nothing to pin",
            newest.record.verdict
        ));
        return Ok(out);
    }
    out.expected = Some(expected.clone());
    if member.state == MemberState::Started && member.cwd.is_dir() {
        let now = Git::new(ctx.platform, &member.cwd).head().ok();
        if let Some(now) = &now
            && *now != expected
        {
            out.problems.push(format!(
                "{role}'s worktree is now at {now}, but its newest verify ran at {expected}: run \
                 `teamwire verify <team> --role {role}` again before you take evidence"
            ));
        }
        out.worktree_head = now;
    }
    if !clone.is_dir() {
        out.problems.push(format!(
            "{} does not exist: clone the repository there first",
            clone.display()
        ));
        return Ok(out);
    }
    match Git::new(ctx.platform, clone).head() {
        Ok(head) => {
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
        Err(e) => out
            .problems
            .push(format!("the clone's HEAD cannot be read: {e}")),
    }
    Ok(out)
}
