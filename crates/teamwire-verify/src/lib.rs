//! Runs each member's check and the diff rules of ADR 0009: the diff base,
//! `owns` matching, the per-language rules in `rules/`, the five verdicts,
//! and waivers bound to a HEAD. Also the report check used by completions.
//!
//! Never: change a member's worktree; count a waiver for a HEAD other than
//! the one it was recorded at; run a check except through `Platform::run`
//! with the member's timeout.
//!
//! Invariants: every verify run writes one record per member with its HEAD
//! sha; `PASS` means the check passed and no flag stands unwaived.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use teamwire_core::{Member, ReportGaps, Role};
use teamwire_store::{Ctx, StoreError};
use thiserror::Error;

const NOT_BUILT: &str = "teamwire-verify is not built yet";

/// What `teamwire verify` was asked to do besides the checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyOpts {
    /// Also run `PRAGMA integrity_check` and check every message hash.
    pub deep: bool,
    /// Rewrite missing or differing `comms/` and `outbox/` copies from the store.
    pub repair_files: bool,
}

/// One member's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verdict {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "FLAGGED")]
    Flagged,
    #[serde(rename = "FAIL")]
    Fail,
    #[serde(rename = "NO-CHECK")]
    NoCheck,
    #[serde(rename = "SKIPPED")]
    Skipped,
}

/// The id of one flag on one member, such as a weakened test at a path and
/// line. A waiver names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FlagId(String);

impl FlagId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FlagId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One flag the diff rules raised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    pub id: FlagId,
    pub detail: String,
    pub waived: bool,
}

/// One member's verify result, as printed and stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyRecord {
    pub role: Role,
    pub verdict: Verdict,
    pub head_sha: Option<String>,
    pub exit_code: Option<i32>,
    pub seconds: Option<u64>,
    pub flags: Vec<Flag>,
    /// `verify/<role>.log`, the full check output.
    pub log: Option<PathBuf>,
    /// Why the member was skipped or had no check.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerifyError {
    #[error("no member {0}")]
    UnknownRole(Role),
    #[error("member has no flag {0} at its current HEAD")]
    UnknownFlag(FlagId),
    #[error("git failed: {0}")]
    Git(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

/// Runs the check and the diff rules for one member, or for every member.
pub fn verify(_ctx: &mut Ctx<'_>, _role: Option<&Role>, _opts: VerifyOpts) -> Vec<VerifyRecord> {
    Vec::new()
}

/// Records a waiver for one flag of one member at its current HEAD.
pub fn waive(
    _ctx: &mut Ctx<'_>,
    _role: &Role,
    _flag: &FlagId,
    _reason: &str,
) -> Result<(), VerifyError> {
    Err(VerifyError::Internal(NOT_BUILT.to_owned()))
}

/// What the member's report file lacks: the file itself, a progress line
/// for each of its own commits since it started, the decisions list.
pub fn report_check(_ctx: &Ctx<'_>, _member: &Member) -> Result<ReportGaps, VerifyError> {
    Err(VerifyError::Internal(NOT_BUILT.to_owned()))
}
