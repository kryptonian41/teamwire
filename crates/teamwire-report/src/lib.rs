//! Completions: `done`, the report check, result files, delivering a result
//! to the lead, the harness-neutral part of the turn-end and in-turn hooks
//! (ADR 0007), and the lead's Stop and tool hooks (ADR 0018, ADR 0035).
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

mod decisions;
mod done;
mod final_message;
mod handoff;
mod lead_handoff;
mod lead_stop;
mod lead_tool;
mod note;
mod paths;
mod received;
mod reminder;
mod result_file;
mod turn_end;
mod usage;

pub use decisions::{
    Block, DecisionsError, DecisionsQuery, DecisionsView, MemberDecisions, RepoScope,
    TeamDecisions, find_blocks, read_decisions, repo_dirs,
};
pub use done::{done, done_with_status, hash_token, parse_status, token_matches};
pub use handoff::handoff_note;
pub use lead_handoff::{lead_handoff_hold, lead_handoff_note};
pub use lead_stop::{afk_hold_notes, teams_without_watch, undelivered_to_lead, watch_missing_note};
pub use lead_tool::lead_tool_note;
pub use received::{Received, mark_received};
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

/// Calls `teamwire_verify::report_check`. A check that errors gives no gaps,
/// so the member is not held for a failure it did not cause (INV-17), and the
/// cause is logged.
pub fn report_gaps(ctx: &Ctx<'_>, member: &Member) -> ReportGaps {
    check_report(ctx, member).0
}

/// `report_gaps`, and the error's text when the check itself failed. A
/// caller that records an event puts the text in its data, so a broken check
/// is not read as "no gaps".
pub(crate) fn check_report(ctx: &Ctx<'_>, member: &Member) -> (ReportGaps, Option<String>) {
    match teamwire_verify::report_check(ctx, member) {
        Ok(gaps) => (gaps, None),
        Err(e) => {
            tracing::warn!(error = %e, role = %member.role, "the report check could not run");
            (ReportGaps::default(), Some(e.to_string()))
        }
    }
}
