//! Runs each member's check and the diff rules of ADR 0009: the diff base,
//! `owns` matching, the per-language rules in `rules/`, the five verdicts,
//! and waivers bound to a HEAD. Also the report check used by completions.
//!
//! Never: change a member's worktree; count a waiver for a HEAD other than
//! the one it was recorded at; run a check except through `Platform::run`
//! with the member's timeout.
//!
//! Three ways to switch a test off stay unflagged, because no line-by-line
//! rule sees them: `#[cfg(not(test))]` (the rules have no path filter, so
//! it would flag production code), a test wrapped in `/* */`, and a net-zero
//! swap of one test for another.
//!
//! Invariants: every verify run writes one record per member with its HEAD
//! sha; `PASS` means the check passed and no flag stands unwaived.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use teamwire_core::{CompletionStatus, Member, MemberKind, MemberState, Mode, Role};
use teamwire_store::{Ctx, NewVerifyRecord, StoreError, Waiver};
use thiserror::Error;

mod check;
mod files;
mod git;
mod inspect;
mod owns;
mod report;
mod rules;

pub use files::{DeepReport, FileFinding, FileProblem, check_copies, deep_check, repair_copies};
pub use report::{report_check, report_path};

use git::Git;
use inspect::{RawFlag, inspect_diff};
use rules::Ruleset;

/// The brief's copy in a worktree. Launch keeps it out of git, and verify
/// ignores it wherever it looks (LCH-24).
pub(crate) const TASK_FILE: &str = "TASK.md";

/// True for the brief's copy: the exact path `TASK.md` at the root of a
/// worktree, and no other file that happens to end in those letters. Close
/// and verify both use this one rule.
#[must_use]
pub fn is_task_file(path: &str) -> bool {
    path == TASK_FILE
}

/// True for a `git status --porcelain` line about the brief's copy alone.
#[must_use]
pub fn is_task_file_status(line: &str) -> bool {
    line.get(3..).is_some_and(is_task_file)
}

/// What `teamwire verify` was asked to do besides the checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyOpts {
    /// Also run `PRAGMA integrity_check` and check every message hash.
    pub deep: bool,
    /// Rewrite missing or differing `comms/` copies from the store, only where the stored body still matches its hash.
    pub repair_files: bool,
}

/// One member's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verdict {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "FLAGGED")]
    Flagged,
    #[serde(rename = "FAIL")]
    Fail,
    #[serde(rename = "NO-CHECK")]
    NoCheck,
    #[serde(rename = "SKIPPED")]
    Skipped,
}

/// The id of one flag on one member, such as a weakened test at a path and
/// line. A waiver names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FlagId(String);

impl FlagId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FlagId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One flag the diff rules raised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    pub id: FlagId,
    pub detail: String,
    pub waived: bool,
}

/// One member's verify result, as printed and stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyRecord {
    pub role: Role,
    pub verdict: Verdict,
    pub head_sha: Option<String>,
    pub exit_code: Option<i32>,
    pub seconds: Option<u64>,
    pub flags: Vec<Flag>,
    /// `verify/<role>.log`, the full check output.
    pub log: Option<PathBuf>,
    /// Why the member was skipped or had no check.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerifyError {
    #[error("no member {0}")]
    UnknownRole(Role),
    #[error("member has no flag {0} at its current HEAD")]
    UnknownFlag(FlagId),
    #[error("git failed: {0}")]
    Git(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

impl Verdict {
    /// The word stored in the verify record and printed.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Flagged => "FLAGGED",
            Self::Fail => "FAIL",
            Self::NoCheck => "NO-CHECK",
            Self::Skipped => "SKIPPED",
        }
    }
}

/// Everything one `verify` run found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub records: Vec<VerifyRecord>,
    /// Copies in `comms/` and `outbox/` that still do not match the store.
    pub copies: Vec<FileFinding>,
    /// Copies `--repair-files` wrote back.
    pub repaired: Vec<PathBuf>,
    /// The `--deep` result, when it was asked for.
    pub deep: Option<DeepReport>,
}

impl VerifyReport {
    /// True when every checked member is `PASS`, no copy is bad and the deep
    /// checks found nothing. A skipped member was not checked, so it does not
    /// count against the run, unless the lead named that member: then the
    /// answer they asked for is "not clean".
    pub fn is_clean(&self, role_named: bool) -> bool {
        self.records.iter().all(|r| match r.verdict {
            Verdict::Pass => true,
            Verdict::Skipped => !role_named,
            Verdict::Flagged | Verdict::Fail | Verdict::NoCheck => false,
        }) && self.copies.is_empty()
            && self.deep.as_ref().is_none_or(DeepReport::is_clean)
    }
}

/// Runs the check and the diff rules for one member, or for every member,
/// stores one record per checked member, and runs the file checks of `opts`.
pub fn run(
    ctx: &mut Ctx<'_>,
    role: Option<&Role>,
    opts: VerifyOpts,
) -> Result<VerifyReport, VerifyError> {
    let members = ctx.store.read(|tx| tx.members()?.all_or_corrupt())?;
    let chosen: Vec<Member> = match role {
        Some(role) => vec![
            members
                .into_iter()
                .find(|m| &m.role == role)
                .ok_or_else(|| VerifyError::UnknownRole(role.clone()))?,
        ],
        None => members,
    };
    let rules = Ruleset::load()?;
    let team_dir = report::team_dir(ctx)?;
    let mut records = Vec::new();
    for member in &chosen {
        records.push(verify_member(ctx, &rules, &team_dir, member)?);
    }
    let repaired = if opts.repair_files {
        repair_copies(ctx)?
    } else {
        Vec::new()
    };
    let copies = check_copies(ctx)?;
    let deep = if opts.deep {
        Some(deep_check(ctx)?)
    } else {
        None
    };
    Ok(VerifyReport {
        records,
        copies,
        repaired,
        deep,
    })
}

/// Records a waiver for one flag of one member at its current HEAD. The flag
/// must be one the diff rules raise for that HEAD right now.
pub fn waive(
    ctx: &mut Ctx<'_>,
    role: &Role,
    flag: &FlagId,
    reason: &str,
) -> Result<(), VerifyError> {
    if reason.trim().is_empty() {
        return Err(VerifyError::Internal("a waiver needs a reason".to_owned()));
    }
    let member = ctx.store.read(|tx| tx.member(role))?;
    let member = member.ok_or_else(|| VerifyError::UnknownRole(role.clone()))?;
    let git = Git::new(ctx.platform, &member.cwd);
    let head = git.head()?;
    let rules = Ruleset::load()?;
    let raised = member_flags(ctx, &rules, &member, &git);
    if !raised.iter().any(|f| f.id == flag.as_str()) {
        return Err(VerifyError::UnknownFlag(flag.clone()));
    }
    let now = ctx.clock.now();
    let waiver = Waiver {
        role: role.clone(),
        flag_id: flag.as_str().to_owned(),
        head_sha: head,
        reason: reason.to_owned(),
        created_at: now,
    };
    ctx.store.write(|tx| tx.insert_waiver(now, &waiver))?;
    Ok(())
}

fn skipped(member: &Member, note: impl Into<String>) -> VerifyRecord {
    VerifyRecord {
        role: member.role.clone(),
        verdict: Verdict::Skipped,
        head_sha: None,
        exit_code: None,
        seconds: None,
        flags: Vec::new(),
        log: None,
        note: Some(note.into()),
    }
}

fn verify_member(
    ctx: &mut Ctx<'_>,
    rules: &Ruleset,
    team_dir: &Path,
    member: &Member,
) -> Result<VerifyRecord, VerifyError> {
    match member.state {
        MemberState::Removed => return Ok(skipped(member, "the member was removed")),
        MemberState::Creating | MemberState::Failed => {
            return Ok(skipped(member, "the member never started"));
        }
        MemberState::Started => {}
    }
    let worktree = member.mode == Mode::Worktree;
    if !worktree && member.check.is_none() {
        return Ok(skipped(member, "a pane member with no check"));
    }
    if !member.cwd.is_dir() {
        // A started worktree member that has lost its folder has lost its
        // work, which is not a clean run.
        let mut record = skipped(member, format!("{} does not exist", member.cwd.display()));
        if worktree {
            record.verdict = Verdict::Fail;
        }
        return Ok(record);
    }
    let git = Git::new(ctx.platform, &member.cwd);
    let head = match git.head() {
        Ok(head) => Some(head),
        Err(e) if worktree => {
            let mut record = skipped(member, e.to_string());
            record.verdict = Verdict::Fail;
            return Ok(record);
        }
        Err(_) => None,
    };
    let raised = if worktree {
        member_flags(ctx, rules, member, &git)
    } else {
        Vec::new()
    };
    let waived: HashSet<String> = match &head {
        Some(head) => ctx
            .store
            .read(|tx| tx.waivers(&member.role, head)?.all_or_corrupt())?
            .into_iter()
            .map(|w| w.flag_id)
            .collect(),
        None => HashSet::new(),
    };
    let mut flags: Vec<Flag> = raised
        .into_iter()
        .map(|f| Flag {
            waived: waived.contains(&f.id),
            id: FlagId::new(f.id),
            detail: f.detail,
        })
        .collect();
    let outcome = member
        .check
        .as_deref()
        .map(|check| check::run_check(ctx, member, check, team_dir));
    // The check may commit. The diff above was read at `head`, so a record at
    // `head` would vouch for commits nobody inspected. No waiver clears this.
    let moved_to = head
        .as_ref()
        .and_then(|before| git.head().ok().filter(|after| after != before));
    if let (Some(before), Some(after)) = (&head, moved_to) {
        flags.push(Flag {
            id: FlagId::new("head-moved"),
            detail: format!(
                "HEAD moved from {before} to {after} while verify ran; run verify again"
            ),
            waived: false,
        });
    }
    let verdict = match &outcome {
        None => Verdict::NoCheck,
        Some(o) if o.exit_code != 0 => Verdict::Fail,
        Some(_) if flags.iter().any(|f| !f.waived) => Verdict::Flagged,
        Some(_) => Verdict::Pass,
    };
    let log_sha256 = outcome
        .as_ref()
        .and_then(|o| o.log.as_ref().map(|(_, sha)| *sha));
    let record = VerifyRecord {
        role: member.role.clone(),
        verdict,
        head_sha: head,
        exit_code: outcome.as_ref().map(|o| o.exit_code),
        seconds: outcome.as_ref().map(|o| o.seconds),
        flags,
        log: outcome
            .as_ref()
            .and_then(|o| o.log.as_ref().map(|(p, _)| p.clone())),
        note: outcome.and_then(|o| o.note),
    };
    let stored = NewVerifyRecord {
        role: record.role.clone(),
        head_sha: record.head_sha.clone().unwrap_or_else(|| "none".to_owned()),
        verdict: verdict.as_str().to_owned(),
        exit_code: record.exit_code,
        seconds: record.seconds,
        flags: record.flags.iter().map(|f| f.id.to_string()).collect(),
        log_sha256,
    };
    let now = ctx.clock.now();
    ctx.store
        .write(|tx| tx.insert_verify_record(now, &stored))?;
    Ok(record)
}

/// An implementer that recorded `done` while `HEAD` is still its base made no
/// commit: its work, if any, is not on its branch.
fn no_commits_flag(ctx: &Ctx<'_>, member: &Member, git: &Git<'_>) -> Option<RawFlag> {
    let base = member.base_sha.as_deref()?;
    if member.kind != MemberKind::Implementer || git.head().ok()?.as_str() != base {
        return None;
    }
    let latest = ctx
        .store
        .read(|tx| tx.latest_completion(&member.role))
        .ok()??;
    (latest.completion.status == CompletionStatus::Done).then(|| {
        RawFlag::new(
            "no-commits",
            "recorded done, but HEAD is still the base: the branch has no commit",
        )
    })
}

/// The flags a worktree member raises now: its diff, its report, and any
/// uncommitted path. Each flag id appears once.
fn member_flags(ctx: &Ctx<'_>, rules: &Ruleset, member: &Member, git: &Git<'_>) -> Vec<RawFlag> {
    let mut flags = Vec::new();
    let start = match (member.base_sha.as_deref(), member.base.as_deref()) {
        (Some(sha), base) => Some(git.diff_base(sha, base)),
        (None, base) => base.map(str::to_owned),
    };
    match start {
        Some(start) => match inspect_diff(git, &start, &member.owns, rules) {
            Ok(found) => flags.extend(found),
            Err(e) => flags.push(RawFlag::new("diff-failed", e.to_string())),
        },
        None => flags.push(RawFlag::new(
            "no-base",
            "no base recorded; the diff was not checked",
        )),
    }
    if member.kind == MemberKind::Implementer && member.owns.is_empty() {
        flags.push(RawFlag::new(
            "no-owns",
            "the implementer owns no path, so no edit is outside its scope",
        ));
    }
    flags.extend(report::report_flags(ctx, member));
    flags.extend(no_commits_flag(ctx, member, git));
    // One flag per path, so a waiver for one stray file does not cover a
    // later edit of another.
    match git.uncommitted() {
        Ok(paths) => flags.extend(paths.into_iter().map(|path| {
            RawFlag::new(
                format!("uncommitted:{path}"),
                format!("uncommitted change: {path}"),
            )
        })),
        Err(e) => flags.push(RawFlag::new("status-failed", e.to_string())),
    }
    let mut seen = HashSet::new();
    flags.retain(|f| seen.insert(f.id.clone()));
    flags
}
