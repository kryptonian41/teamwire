//! The team spec that `teamwire launch` reads, as data. Validation belongs
//! to `teamwire-launch`; these types only carry what the JSON said.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::member::{MemberKind, Mode};
use crate::names::{HarnessId, HostId, Role, TeamName};

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSpec {
    pub team: TeamName,
    pub title: String,
    #[serde(default)]
    pub setup: Option<String>,
    #[serde(default)]
    pub harness: Option<HarnessId>,
    #[serde(default)]
    pub host: Option<HostId>,
    pub children: Vec<MemberSpec>,
    /// Claude plugin id `name@marketplace` to on/off, written into the member's
    /// per-run settings. A member's value wins over the team's, per key.
    #[serde(default)]
    pub plugins: BTreeMap<String, bool>,
    /// Tickets the team works on (ADR 0026).
    #[serde(default)]
    pub refs: Vec<TicketRef>,
}

/// The most refs a team holds after any `--add` (ADR 0026). `validate` checks
/// it for a dry run, and `Tx::add_refs` checks it again under the write lock.
pub const REFS_MAX: usize = 20;

/// A ticket in some tracker. teamwire assumes no provider, so both fields are
/// required. It never looks the ticket up: `validate` only checks the shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketRef {
    /// The tracker's lowercase name, such as `linear`, `github` or `jira`.
    pub provider: String,
    /// The ticket's id in that tracker, such as `GKA-580`.
    pub id: String,
}

impl fmt::Display for TicketRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberSpec {
    pub role: Role,
    pub title: String,
    #[serde(default)]
    pub kind: Option<MemberKind>,
    pub mode: Mode,
    #[serde(default)]
    pub harness: Option<HarnessId>,
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub owns: Vec<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub task_file: Option<PathBuf>,
    #[serde(default)]
    pub deadline_s: Option<u64>,
    #[serde(default)]
    pub setup: Option<String>,
    #[serde(default = "yes")]
    pub dispatch: bool,
    #[serde(default = "yes")]
    pub accept_trust: bool,
    /// Claude plugin id `name@marketplace` to on/off, written into the member's
    /// per-run settings. A member's value wins over the team's, per key.
    #[serde(default)]
    pub plugins: BTreeMap<String, bool>,
}
