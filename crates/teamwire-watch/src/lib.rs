//! The watch loop's single poll (WAT-1..WAT-16) and the read-only status
//! view. Both derive each member's state the same way.
//!
//! Never: call a writing host method; print more than one line per poll;
//! announce the same key twice (a restarted watch prints
//! its `WATCHING` line again, and `TEAM-COMPLETE` for a finished team); write anything from `status` (WAT-11).
//!
//! Invariants: announcements are stored before they are printed, and each
//! key carries its generation or episode number; `TEAM-COMPLETE` is printed
//! once per watch, when every live member is settled. A restarted watch
//! prints it again for a finished team, and `WATCHING` on its first poll;
//! both are intended.

mod apply;
mod lease;
mod memory;
mod observe;
mod plan;
mod poll;
mod snapshot;
mod status;
mod text;

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use teamwire_core::{
    Actor, DeliveryState, DisplayId, Liveness, MessageType, Pid, Recipient, ReportState, Role,
    SessionId, TeamName, TeamState, Timestamp,
};
use teamwire_store::Ctx;

pub use lease::{WatchError, start, stop};
pub use memory::{WatchEnd, WatchMemory};
pub use poll::poll;

/// A delivery handed to the host and still not acknowledged after this long
/// counts as unread (ADR 0003 point 10).
pub const UNREAD_AFTER: Duration = Duration::from_secs(600);
/// The watch tells the lead about a question still unanswered after this long.
pub const UNANSWERED_AFTER: Duration = Duration::from_secs(600);

/// The lead as `status` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadView {
    pub session_id: Option<SessionId>,
    pub liveness: Liveness,
}

/// One member as `status` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberView {
    pub role: Role,
    pub title: String,
    /// `done(<status>)`, `unreported`, `undelivered`, `died?`, `blocked`,
    /// `working`, `quiet`, `late` or `prompted outside teamwire`.
    pub state: String,
    pub generation: u32,
    pub age_s: u64,
    pub deadline_s: u64,
    /// The teamwire version that started this member, from its
    /// `member.started` event. `None` before it started.
    pub started_by: Option<String>,
    /// True when `started_by` differs from the running teamwire.
    pub older_build: bool,
    /// The delivery state of its newest result.
    pub result: Option<ReportState>,
    pub branch: Option<String>,
    /// With `--role`: the command that resumes this member's session with
    /// its per-run configuration.
    pub resume_command: Option<String>,
}

/// One of the last messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub display_id: DisplayId,
    pub time: Timestamp,
    #[serde(rename = "type")]
    pub kind: MessageType,
    pub from: Actor,
    pub to: Vec<Recipient>,
    pub excerpt: String,
}

/// A delivery that needs attention: `pending` over 60 s, `unconfirmed` or
/// `undelivered`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StuckDelivery {
    pub display_id: DisplayId,
    pub to: Recipient,
    pub state: DeliveryState,
    pub age_s: u64,
}

/// A delivery handed to its recipient's session that nobody acknowledged
/// within `UNREAD_AFTER`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnreadDelivery {
    pub display_id: DisplayId,
    pub to: Recipient,
    pub age_s: u64,
}

/// A question that no answer points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnansweredQuestion {
    pub display_id: DisplayId,
    pub from: Actor,
    pub to: Vec<Recipient>,
    pub age_s: u64,
}

/// A result the lead has not received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeliveredResult {
    pub role: Role,
    pub n: u32,
    pub report_state: ReportState,
    pub path: PathBuf,
}

/// Whether a watch is looking after the team.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    /// A live process holds the lease and beats recently.
    Running,
    /// A lease row exists, but its process is gone or its heartbeat is old.
    Stale,
    /// No lease row: no watch was started, or it ended.
    #[default]
    None,
}

/// The team's watch as `status` shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchView {
    pub state: WatchState,
    pub pid: Option<Pid>,
    /// Seconds since the last heartbeat.
    pub heartbeat_age_s: Option<u64>,
}

/// `teamwire status <team>` and `team_status`. Read-only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusView {
    pub team: Option<TeamName>,
    pub title: Option<String>,
    pub state: Option<TeamState>,
    /// The tickets the team works on; empty when it has none.
    pub refs: Vec<String>,
    pub lead: Option<LeadView>,
    pub watch: WatchView,
    pub members: Vec<MemberView>,
    /// The last five messages.
    pub messages: Vec<MessageView>,
    pub stuck_deliveries: Vec<StuckDelivery>,
    pub unread_deliveries: Vec<UnreadDelivery>,
    pub unanswered_questions: Vec<UnansweredQuestion>,
    pub undelivered_results: Vec<UndeliveredResult>,
    /// Why part of the view could not be read.
    pub errors: Vec<String>,
}

/// The status view. Reads only.
pub fn status(ctx: &Ctx<'_>) -> StatusView {
    status::status_with(ctx, None)
}

pub use status::status_with;
