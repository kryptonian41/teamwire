//! Whole-team close and single-member removal, in the fixed order of CLS-1,
//! under the close lock, with the archive (`SQLite` backup API and
//! `SHA256SUMS`) and the sleep-hold release last.
//!
//! Never: remove a worktree with force; delete a branch; remove a busy
//! member without `--stop`; remove an unverified implementer without
//! `--unverified` (CLS-5); guess when a member's busy state is unknown.
//!
//! Invariants: every step re-checks its condition under the close lock; a
//! step that fails stops the close and leaves state the next run can read.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use teamwire_core::{EventType, Role};
use teamwire_store::Ctx;

mod archive;
mod delete;
mod facts;
mod git;
mod hold;
mod member_close;
mod plan;
mod remove;
mod team_close;

pub use plan::plan_close_with;

/// The whole team, or one member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", content = "role", rename_all = "snake_case")]
pub enum CloseTarget {
    Team,
    Member(Role),
}

/// The flags of `teamwire close`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each field is one independent command-line flag of `teamwire close`"
)]
pub struct CloseOpts {
    /// Allow removing a busy member.
    pub stop: bool,
    /// Override CLS-5 and print why.
    pub unverified: bool,
    /// Leave `$TEAMWIRE_HOME/teams/<team>` in place, marked closed.
    pub keep_state: bool,
    /// Where the archive goes instead of `$TEAMWIRE_HOME/archive/<team>-<date>`.
    pub archive_to: Option<PathBuf>,
    /// Clear a close lock whose owner pid is dead, and nothing else.
    pub clear_stale_lock: bool,
}

/// One step of a close, as planned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseStep {
    pub step: String,
    pub detail: String,
}

/// What a close would do (`--dry-run`), or why it would refuse.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePlan {
    pub steps: Vec<CloseStep>,
    pub refusals: Vec<String>,
}

/// How one step went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepOutcome {
    pub step: String,
    pub ok: bool,
    pub detail: String,
}

/// What a close did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseReport {
    pub steps: Vec<StepOutcome>,
    /// The step the close stopped at, if it did not finish.
    pub stopped_at: Option<String>,
    /// Why nothing was done: a rule refused before any step ran (exit 1).
    pub refusals: Vec<String>,
    pub archive: Option<PathBuf>,
    pub branches_kept: Vec<String>,
    /// Commits on kept branches that are not pushed anywhere.
    pub unpushed: Vec<String>,
}

/// The plan for `--dry-run`, with no flag given. Changes nothing.
pub fn plan_close(ctx: &Ctx<'_>, target: CloseTarget) -> ClosePlan {
    plan_close_with(ctx, target, &CloseOpts::default())
}

pub fn close(ctx: &mut Ctx<'_>, target: CloseTarget, opts: CloseOpts) -> CloseReport {
    close_observed(ctx, target, opts, &|_| {})
}

/// `close`, calling `observer` with each step's name right before the step
/// runs and before its re-check. Tests use it to change the world between
/// two steps, which is the only way to reach the re-checks of CLS-3.
pub fn close_observed(
    ctx: &mut Ctx<'_>,
    target: CloseTarget,
    opts: CloseOpts,
    observer: &dyn Fn(&str),
) -> CloseReport {
    if opts.clear_stale_lock {
        return team_close::clear_stale_lock(ctx);
    }
    let plan = plan_close_with(ctx, target.clone(), &opts);
    if !plan.refusals.is_empty() {
        record_refusal(ctx, &plan.refusals);
        return CloseReport {
            refusals: plan.refusals,
            ..CloseReport::default()
        };
    }
    match target {
        CloseTarget::Team => team_close::run(ctx, opts, observer),
        CloseTarget::Member(role) => remove::run(ctx, &role, &opts, observer),
    }
}

/// `close.refused`, so the metrics can count where closes stop. A refusal
/// changes nothing else; a store that cannot take the event does not turn a
/// refusal into a failure.
fn record_refusal(ctx: &mut Ctx<'_>, reasons: &[String]) {
    let now = ctx.clock.now();
    let data = serde_json::json!({ "reason": reasons.join("; ") });
    if let Err(e) = ctx
        .store
        .write(|tx| tx.record_event(now, EventType::CloseRefused, None, None, &data))
    {
        tracing::warn!(error = %e, "close.refused could not be recorded");
    }
}
