//! Launch: whole-spec validation before anything is created, worktrees,
//! setup, the contract text (data files in `contracts/`), member tokens,
//! spawning through each member's host, the trust step, the dispatch
//! message, and the sleep hold.
//!
//! Never: create anything before the whole spec is valid; start a member
//! that has no identity channel; reuse a role unless its earlier launch
//! failed before it started; write a token into a worktree.
//!
//! Invariants: a member is recorded `creating` before its first
//! irreversible step and `started` after the agent runs, so close finds
//! everything whatever step a crash hit (LCH-3).

use serde::{Deserialize, Serialize};
use teamwire_core::{Role, TeamSpec};
use teamwire_store::Ctx;

const NOT_BUILT: &str = "teamwire-launch is not built yet";

/// A spec that passed every check, ready to launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidPlan {
    pub spec: TeamSpec,
    /// `--add`: members join a running team.
    pub add: bool,
    /// The warnings of LCH-9, LCH-10 and LCH-18, printed before launching.
    pub warnings: Vec<String>,
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

/// Checks the whole spec, and every path the launch will touch (LCH-2).
pub fn validate(_spec: &TeamSpec, _add: bool, _ctx: &Ctx<'_>) -> Result<ValidPlan, Vec<SpecError>> {
    Err(vec![SpecError {
        role: None,
        field: None,
        code: "not_built".to_owned(),
        message: NOT_BUILT.to_owned(),
    }])
}

/// Launches every member of `plan`.
pub fn launch(_ctx: &mut Ctx<'_>, plan: ValidPlan) -> LaunchReport {
    LaunchReport {
        members: plan
            .spec
            .children
            .into_iter()
            .map(|child| MemberOutcome {
                role: child.role,
                outcome: LaunchOutcome::Failed(NOT_BUILT.to_owned()),
            })
            .collect(),
        warnings: plan.warnings,
        watch_command: format!("teamwire watch {}", plan.spec.team),
    }
}
