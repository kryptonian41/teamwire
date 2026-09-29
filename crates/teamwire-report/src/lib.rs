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
use teamwire_core::{CompletionStatus, Member, ReportGaps, Role, Ulid};
use teamwire_store::{Ctx, StoreError};
use thiserror::Error;

mod done;
mod final_message;
mod note;
mod paths;
mod reminder;
mod result_file;
mod turn_end;

pub use done::{done, done_with_status, hash_token, parse_status, token_matches};
pub use reminder::on_tool_use;
pub use turn_end::on_turn_end;

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

/// Calls `teamwire_verify::report_check`; errors → empty gaps.
pub fn report_gaps(ctx: &Ctx<'_>, member: &Member) -> ReportGaps {
    teamwire_verify::report_check(ctx, member).unwrap_or_default()
}
