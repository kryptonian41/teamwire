//! The team spec that `teamwire launch` reads, as data. Validation belongs
//! to `teamwire-launch`; these types only carry what the JSON said.

use std::collections::BTreeMap;
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
    /// Tokens of context at which a member is told to hand off. A member's
    /// value wins over the team's. Without one, the threshold is
    /// `DEFAULT_HANDOFF_PCT` of the model's window. Launch refuses 0.
    #[serde(default)]
    pub context_handoff_tokens: Option<u64>,
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
    /// Tokens of context at which a member is told to hand off. A member's
    /// value wins over the team's. Without one, the threshold is
    /// `DEFAULT_HANDOFF_PCT` of the model's window. Launch refuses 0.
    #[serde(default)]
    pub context_handoff_tokens: Option<u64>,
}
