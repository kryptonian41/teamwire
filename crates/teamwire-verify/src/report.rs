//! The report check (ADR 0007 point 7, REP-5, REP-6): does the member's own
//! report file say what the contract asked for?

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::{Member, MemberKind, Mode, ReportGaps, Role};
use teamwire_store::Ctx;

use crate::VerifyError;
use crate::git::Git;
use crate::inspect::RawFlag;

/// A member's clock may run a little behind the store's, so a commit made at
/// the very start of a generation still counts (INV-19).
const CLOCK_SLACK_S: i64 = 120;

/// Each git call of the report check. Claude kills the Stop hook at 60 s and
/// delivery has a 40 s budget, so a hung git must fail fast: the error means
/// "no gaps" to the caller (REP-5).
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// The team's state directory, `$TEAMWIRE_HOME/teams/<team>`.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub(crate) fn team_dir(ctx: &Ctx<'_>) -> Result<PathBuf, VerifyError> {
    let team = ctx.store.read(|tx| tx.team())?;
    let team = team.ok_or_else(|| VerifyError::Internal("the store has no team row".to_owned()))?;
    Ok(ctx.home.join("teams").join(team.name.as_str()))
}

/// `reports/<role>.md` under the team directory: the file the report check
/// reads, and the one a hold note must name.
pub fn report_path(team_dir: &Path, role: &Role) -> PathBuf {
    team_dir
        .join("reports")
        .join(format!("{}.md", role.as_str()))
}

/// What the member's report file lacks: the file itself, a progress line for
/// each of its own commits since its generation started, the decisions list.
///
/// A commit is the member's own when it is first-parent, not a merge, and
/// authored no earlier than 120 s before the generation began. A merged
/// contract amendment and a rebased stack both bring in commits that fail one
/// of those tests, and the member wrote no line for work it never did.
pub fn report_check(ctx: &Ctx<'_>, member: &Member) -> Result<ReportGaps, VerifyError> {
    let path = report_path(&team_dir(ctx)?, &member.role);
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(ReportGaps {
            report_missing: true,
            ..ReportGaps::default()
        });
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut gaps = ReportGaps::default();
    if member.kind == MemberKind::Implementer
        && member.mode == Mode::Worktree
        && let Some(base) = member.base_sha.as_deref()
        && member.cwd.is_dir()
    {
        let git = Git::with_timeout(ctx.platform, &member.cwd, GIT_TIMEOUT);
        let since = member.gen_started_at.unix_millis().div_euclid(1000) - CLOCK_SLACK_S;
        gaps.commits_without_line = git
            .own_commits(base)?
            .into_iter()
            .filter(|(_, authored)| *authored >= since)
            .map(|(sha, _)| sha.chars().take(7).collect::<String>())
            .filter(|short| !text.contains(short.as_str()))
            .collect();
    }
    gaps.decisions_missing = !text.to_lowercase().contains("decisions i made");
    Ok(gaps)
}

/// The report's gaps as flags, for `verify`.
pub(crate) fn report_flags(ctx: &Ctx<'_>, member: &Member) -> Vec<RawFlag> {
    let gaps = match report_check(ctx, member) {
        Ok(gaps) => gaps,
        Err(e) => return vec![RawFlag::new("report-unchecked", e.to_string())],
    };
    let mut flags = Vec::new();
    if gaps.report_missing {
        flags.push(RawFlag::new("report-missing", "no report file"));
    }
    for sha in &gaps.commits_without_line {
        flags.push(RawFlag::new(
            format!("report-no-line:{sha}"),
            format!("report has no progress line for commit {sha}"),
        ));
    }
    if gaps.decisions_missing {
        flags.push(RawFlag::new(
            "report-no-decisions",
            "report has no 'Decisions I made' list",
        ));
    }
    flags
}
