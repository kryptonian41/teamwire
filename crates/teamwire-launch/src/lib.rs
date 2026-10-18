//! Launch: whole-spec validation before anything is created, worktrees,
//! setup, the contract text (data files in `contracts/`), member tokens,
//! spawning through each member's host, the trust step, the dispatch
//! message, and the sleep hold. `reopen` resumes the sessions of a team whose
//! sessions are gone.
//!
//! Never: create anything before the whole spec is valid; start a member
//! that has no identity channel; reuse a role unless its earlier launch
//! failed before it started; write a token into a worktree.
//!
//! Invariants: a member is recorded `creating` before its first
//! irreversible step and `started` after the agent runs, so close finds
//! everything whatever step a crash hit (LCH-3).

mod contract;
mod env;
mod git;
mod hold;
mod member;
mod member_parts;
mod member_steps;
mod plan;
mod plan_rules;
mod plan_worktree;
mod reopen;
mod reopen_lead;
mod spec;
mod validate;
mod worktree;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use teamwire_core::{
    Address, HarnessId, HostId, IdentityChannel, LeadRoute, MemberKind, MemberSpec, Role,
    SessionId, SessionKind, TeamName, TeamSpec,
};
use teamwire_store::Ctx;

pub use contract::{ContractParts, render_contract};
pub use env::ValidateEnv;
pub use reopen::{ReopenError, ReopenOpts, ReopenOutcome, ReopenReport, reopen};
pub use spec::parse_spec;
pub use validate::validate;

/// The lead of the team a launch belongs to, as validation resolved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadPlan {
    pub harness: HarnessId,
    pub host: HostId,
    pub route: LeadRoute,
    pub session: Option<(SessionId, SessionKind)>,
}

/// Where a worktree member works and how its branch is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreePlan {
    pub repo: PathBuf,
    pub branch: String,
    /// `<repo>/.worktrees/<slug>`.
    pub path: PathBuf,
    /// The base the spec named. Ignored when the branch already exists.
    pub base: Option<String>,
    pub branch_exists: bool,
    /// The path is a worktree of this branch that an earlier, failed launch
    /// left behind with no commits past its start; launch uses it as it is.
    #[serde(default)]
    pub reuse: bool,
}

/// One member of a valid spec, with everything validation worked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedMember {
    /// The spec entry, with the title cleaned of extra white space.
    pub spec: MemberSpec,
    pub kind: MemberKind,
    pub harness: HarnessId,
    pub host: HostId,
    pub address: Address,
    pub channel: IdentityChannel,
    /// The directory the agent starts in.
    pub cwd: PathBuf,
    pub worktree: Option<WorktreePlan>,
    /// The role's row is `failed` from an earlier launch that never started.
    pub relaunch: bool,
    pub deadline_s: u64,
    /// The brief's text, read while validating.
    pub brief: String,
}

/// A spec that passed every check, ready to launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidPlan {
    pub spec: TeamSpec,
    /// `--add`: members join a running team.
    pub add: bool,
    /// The warnings of LCH-9, LCH-10 and LCH-18, printed before launching.
    pub warnings: Vec<String>,
    pub lead: LeadPlan,
    pub members: Vec<PlannedMember>,
    /// The `teamwire` binary the members' hooks call.
    pub teamwire_bin: PathBuf,
}

/// One problem with a spec. Validation reports all of them at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecError {
    /// The member's role as written in the spec, when the problem is one
    /// member's. Raw text, because the role itself may be what is wrong.
    pub role: Option<String>,
    pub field: Option<String>,
    /// A stable code, such as `no_identity_channel` or `no_host`.
    pub code: String,
    pub message: String,
}

impl SpecError {
    pub(crate) fn new(
        role: Option<&str>,
        field: Option<&str>,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            role: role.map(str::to_owned),
            field: field.map(str::to_owned),
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

/// How one member's launch ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "reason", rename_all = "snake_case")]
pub enum LaunchOutcome {
    Started,
    Refused(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberOutcome {
    pub role: Role,
    #[serde(flatten)]
    pub outcome: LaunchOutcome,
}

/// Per-member outcomes, the warnings, and the watch command to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchReport {
    pub members: Vec<MemberOutcome>,
    pub warnings: Vec<String>,
    pub watch_command: String,
}

/// The team's watch command, printed after a launch. A lead that cannot read
/// a command's output as it is printed (ADR 0023) starts it in the
/// background, and the watch types each line into the lead's pane. A lead
/// whose harness is unknown to the registry gets the plain command.
pub(crate) fn watch_command(
    registry: &teamwire_core::Registry,
    lead: &HarnessId,
    team: &TeamName,
) -> String {
    let reads = registry
        .harness(lead)
        .map_or(true, |h| h.capabilities().reads_watch_lines);
    if reads {
        format!("teamwire watch {team}")
    } else {
        format!("nohup teamwire watch {team} --deliver >/dev/null 2>&1 &")
    }
}

/// Launches every member of `plan`, then starts or reuses the sleep hold.
/// One member's failure never stops the others (LCH-4).
#[expect(
    clippy::needless_pass_by_value,
    reason = "the agreed signature (docs/api/core.md) hands the plan over"
)]
pub fn launch(ctx: &mut Ctx<'_>, plan: ValidPlan) -> LaunchReport {
    member::launch_all(ctx, &plan)
}

#[cfg(test)]
mod tests;
