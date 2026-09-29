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
