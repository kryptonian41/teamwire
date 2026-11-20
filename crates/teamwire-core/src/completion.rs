//! Completions: a member saying it is done, and how its result reached the lead.

use serde::{Deserialize, Serialize};

use crate::message::{DeliveryError, Route};
use crate::names::{Role, Ulid};
use crate::strings::string_enum;
use crate::timestamp::Timestamp;

string_enum! {
    pub enum CompletionStatus {
        Done => "done",
        Blocked => "blocked",
        Failed => "failed",
    }
}

string_enum! {
    pub enum ReportState {
        Pending => "pending",
        Delivering => "delivering",
        Delivered => "delivered",
        Undelivered => "undelivered",
        Unconfirmed => "unconfirmed",
    }
}

string_enum! {
    /// Where the member's final message came from.
    pub enum FinalSource {
        Hook => "hook",
        Transcript => "transcript",
        None => "none",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    pub ulid: Ulid,
    pub role: Role,
    pub generation: u32,
    pub n: u32,
    pub status: CompletionStatus,
    pub summary: String,
    pub final_message: Option<String>,
    pub final_source: FinalSource,
    pub report_state: ReportState,
    pub route: Option<Route>,
    pub reason: Option<DeliveryError>,
    pub created_at: Timestamp,
}

/// What the report check found missing from a member's report file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportGaps {
    pub report_missing: bool,
    /// 7-character shas.
    pub commits_without_line: Vec<String>,
    pub decisions_missing: bool,
}

impl ReportGaps {
    pub fn is_empty(&self) -> bool {
        !self.report_missing && self.commits_without_line.is_empty() && !self.decisions_missing
    }
}

/// A `blocked` result with this summary is a question round (WAT-22): after
/// an optional `handoff: `, it matches `^[1-9][0-9]* questions? for the
/// parent\.`. The member waits for the lead's answers, so it is not done.
#[must_use]
pub fn asks_the_parent(summary: &str) -> bool {
    let s = summary.strip_prefix("handoff: ").unwrap_or(summary);
    let rest = s.trim_start_matches(|c: char| c.is_ascii_digit());
    rest.len() < s.len()
        && !s.starts_with('0')
        && [" questions for the parent.", " question for the parent."]
            .iter()
            .any(|tail| rest.starts_with(tail))
}

/// A result whose summary starts `handoff: `: the member asked for a fresh
/// session, and its next session carries on (ADR 0043).
#[must_use]
pub fn is_handoff(summary: &str) -> bool {
    summary.starts_with("handoff: ")
}
