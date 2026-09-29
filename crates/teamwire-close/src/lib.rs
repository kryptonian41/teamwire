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
use teamwire_core::Role;
use teamwire_store::Ctx;

const NOT_BUILT: &str = "teamwire-close is not built yet";

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
    pub archive: Option<PathBuf>,
    pub branches_kept: Vec<String>,
    /// Commits on kept branches that are not pushed anywhere.
    pub unpushed: Vec<String>,
}

/// The plan for `--dry-run`. Changes nothing.
pub fn plan_close(_ctx: &Ctx<'_>, _target: CloseTarget) -> ClosePlan {
    ClosePlan {
        steps: Vec::new(),
        refusals: vec![NOT_BUILT.to_owned()],
    }
}

pub fn close(_ctx: &mut Ctx<'_>, _target: CloseTarget, _opts: CloseOpts) -> CloseReport {
    CloseReport {
        stopped_at: Some(NOT_BUILT.to_owned()),
        ..CloseReport::default()
    }
}
