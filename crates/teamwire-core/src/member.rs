//! A team member as the store records it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::names::{HarnessId, HostId, Place, Role, SessionId};
use crate::strings::string_enum;
use crate::timestamp::Timestamp;

string_enum! {
    pub enum MemberKind {
        Implementer => "implementer",
        Reviewer => "reviewer",
        Investigator => "investigator",
        Closer => "closer",
    }
}

string_enum! {
    pub enum Mode {
        Worktree => "worktree",
        Pane => "pane",
    }
}

string_enum! {
    pub enum MemberState {
        Creating => "creating",
        Started => "started",
        Failed => "failed",
        Removed => "removed",
    }
}

/// One row of the `members` table. The token hash stays in the store: a
/// `Member` is printed and passed around, and must never carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub role: Role,
    pub title: String,
    pub kind: MemberKind,
    pub mode: Mode,
    pub harness: HarnessId,
    pub host: HostId,
    pub place: Option<Place>,
    pub session_id: Option<SessionId>,
    pub cwd: PathBuf,
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub base_sha: Option<String>,
    pub check: Option<String>,
    pub owns: Vec<String>,
    pub deadline_s: u64,
    pub state: MemberState,
    pub generation: u32,
    pub gen_started_at: Timestamp,
    /// The generation the report check last held this member in, if any.
    pub gate_told_generation: Option<u32>,
    pub removed_at: Option<Timestamp>,
    pub removed_note: Option<String>,
}
