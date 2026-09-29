//! Completions: `done`, the report check, result files, delivering a result
//! to the lead, and the harness-neutral part of the turn-end and in-turn
//! hooks (ADR 0007).
//!
//! Never: fail a hook (every hook path returns a reply, never an error);
//! hold a member twice in one generation for the same report gaps; type a
//! result except through `teamwire-deliver`.
//!
//! Invariants: a completion is stored before any delivery; it is marked
//! `delivering` before the send, and a crash leaves at most one duplicate;
//! the hook's delivery ends inside `Config::hook_budget_s`.

use serde::{Deserialize, Serialize};
use teamwire_core::{CompletionStatus, Member, ReportGaps, Role, TurnEnd, TurnEndReply, Ulid};
use teamwire_store::{Ctx, StoreError};
use thiserror::Error;

const NOT_BUILT: &str = "teamwire-report is not built yet";

/// The output of `teamwire done` and `team_done` (`docs/api/mcp.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DoneReport {
    /// The completion was stored.
    Stored {
        completion: Ulid,
        n: u32,
        generation: u32,
        status: CompletionStatus,
        report: String,
    },
    /// A harness without a turn-end hook, first call with report gaps:
    /// nothing was stored. `stored` is always false.
    NotStored {
        stored: bool,
        gaps: ReportGaps,
        report: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DoneError {
    /// `invalid.status` and the other malformed inputs.
    #[error("invalid.{field}: {message}")]
    Invalid { field: String, message: String },
    #[error("refused.team_closed: the team is closing or closed")]
    TeamClosed,
    #[error("no member {0}")]
    UnknownRole(Role),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

/// Records a completion for `me`.
pub fn done(
    _ctx: &mut Ctx<'_>,
    _me: &Role,
    _status: CompletionStatus,
    _summary: &str,
) -> Result<DoneReport, DoneError> {
    Err(DoneError::Internal(NOT_BUILT.to_owned()))
}

/// Calls `teamwire_verify::report_check`; errors → empty gaps.
pub fn report_gaps(ctx: &Ctx<'_>, member: &Member) -> ReportGaps {
    teamwire_verify::report_check(ctx, member).unwrap_or_default()
}

/// Route A of ADR 0007: hold once for gaps or unread messages, else write
/// the result and deliver it.
pub fn on_turn_end(_ctx: &mut Ctx<'_>, _me: &Role, _t: &TurnEnd) -> TurnEndReply {
    TurnEndReply::Continue
}

/// The reminder of unread message ids for the in-turn hook, once per id.
pub fn on_tool_use(_ctx: &mut Ctx<'_>, _me: &Role) -> Option<String> {
    None
}
