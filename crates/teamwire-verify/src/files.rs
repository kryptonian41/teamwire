//! The readable copies beside the database (STO-10) and the deeper database
//! checks of `--deep` (STO-13). The store holds the original of everything
//! here; verify only compares and, when asked, rewrites the copies.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use teamwire_core::DisplayId;
use teamwire_store::{Ctx, MessageFilter};

use crate::VerifyError;
use crate::report::team_dir;

/// What is wrong with one copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileProblem {
    /// The file does not exist.
    Missing,
    /// The file exists and its SHA-256 is not the one the store holds.
    Differs,
}

/// One copy that does not match the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFinding {
    pub path: PathBuf,
    pub problem: FileProblem,
    /// Whether `--repair-files` can write it back from the store.
    pub repairable: bool,
}

/// The result of `--deep`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepReport {
    /// `ok`, or the problems `PRAGMA integrity_check` listed.
    pub integrity: String,
    /// Messages whose body no longer matches its stored SHA-256.
    pub bad_hashes: Vec<DisplayId>,
}

impl DeepReport {
    pub fn is_clean(&self) -> bool {
        self.integrity == "ok" && self.bad_hashes.is_empty()
    }
}

/// One copy the store can vouch for: where it lives, and its SHA-256.
struct Mirror {
    path: PathBuf,
    sha256: [u8; 32],
    /// What to write back. `None` when the store does not hold the bytes.
    bytes: Option<Vec<u8>>,
    /// A copy that exists but differs is only rewritten when the store holds
    /// its exact bytes; a result file it cannot rebuild is left as found.
    rewrite_differing: bool,
}

fn copies(ctx: &Ctx<'_>, team_dir: &Path) -> Result<Vec<Mirror>, VerifyError> {
    let messages = ctx
        .store
        .read(|tx| tx.messages(&MessageFilter::default())?.all_or_corrupt())?;
    let mut out: Vec<Mirror> = messages
        .into_iter()
        .map(|m| Mirror {
            path: team_dir.join("comms").join(format!("{}.md", m.display_id)),
            sha256: m.body_sha256,
            bytes: Some(m.body.into_bytes()),
            rewrite_differing: true,
        })
        .collect();
    let members = ctx.store.read(|tx| tx.members()?.all_or_corrupt())?;
    for member in members {
        let latest = ctx.store.read(|tx| tx.latest_completion(&member.role))?;
        if let Some(record) = latest
            && let Some(sha256) = record.result_sha256
        {
            out.push(Mirror {
                path: team_dir
                    .join("outbox")
                    .join(format!("{}.md", member.role.as_str())),
                sha256,
                bytes: None,
                rewrite_differing: false,
            });
        }
    }
    Ok(out)
}

fn problem(copy: &Mirror) -> Option<FileProblem> {
    match std::fs::read(&copy.path) {
        Ok(bytes) if <[u8; 32]>::from(Sha256::digest(&bytes)) == copy.sha256 => None,
        Ok(_) => Some(FileProblem::Differs),
        Err(_) => Some(FileProblem::Missing),
    }
}

/// Every copy that is missing or differs from the store.
pub fn check_copies(ctx: &Ctx<'_>) -> Result<Vec<FileFinding>, VerifyError> {
    let dir = team_dir(ctx)?;
    Ok(copies(ctx, &dir)?
        .iter()
        .filter_map(|c| {
            problem(c).map(|p| FileFinding {
                path: c.path.clone(),
                problem: p,
                repairable: c.bytes.is_some(),
            })
        })
        .collect())
}

/// Rewrites what `check_copies` found, from the store, through
/// `Platform::write_atomic`. Returns the paths written. A message copy is
/// written back byte for byte. A result file is not: the store holds the
/// hash of the file the report step wrote, not its bytes, so a missing one
/// cannot come back, and a different one is left for the lead to read.
pub fn repair_copies(ctx: &Ctx<'_>) -> Result<Vec<PathBuf>, VerifyError> {
    let dir = team_dir(ctx)?;
    let mut written = Vec::new();
    for copy in copies(ctx, &dir)? {
        let Some(found) = problem(&copy) else {
            continue;
        };
        let Some(bytes) = &copy.bytes else {
            continue;
        };
        if found == FileProblem::Differs && !copy.rewrite_differing {
            continue;
        }
        if let Some(parent) = copy.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| VerifyError::Internal(format!("{}: {e}", parent.display())))?;
        }
        ctx.platform
            .write_atomic(&copy.path, bytes)
            .map_err(|e| VerifyError::Internal(format!("{}: {e}", copy.path.display())))?;
        written.push(copy.path);
    }
    Ok(written)
}

/// `PRAGMA integrity_check`, and every message body against its hash.
pub fn deep_check(ctx: &Ctx<'_>) -> Result<DeepReport, VerifyError> {
    let integrity = ctx.store.integrity_check()?;
    let messages = ctx
        .store
        .read(|tx| tx.messages(&MessageFilter::default())?.all_or_corrupt())?;
    let bad_hashes = messages
        .into_iter()
        .filter(|m| <[u8; 32]>::from(Sha256::digest(m.body.as_bytes())) != m.body_sha256)
        .map(|m| m.display_id)
        .collect();
    Ok(DeepReport {
        integrity,
        bad_hashes,
    })
}
