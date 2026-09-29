//! The readable copies beside the database (STO-10) and the deeper database
//! checks of `--deep` (STO-13). The store holds the original of everything
//! here; verify only compares and, when asked, rewrites the copies.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use teamwire_core::DisplayId;
use teamwire_store::{Ctx, StoreError};

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

/// Every message with its body and the hash the store kept. Read straight
/// from the table: the store's own listing refuses a row whose body fails its
/// hash, and that is exactly the row these checks must still see.
fn stored_messages(ctx: &Ctx<'_>) -> Result<Vec<(DisplayId, String, Vec<u8>)>, VerifyError> {
    let rows = ctx.store.read(|tx| {
        let mut stmt = tx
            .connection()
            .prepare("SELECT display_id, body, body_sha256 FROM messages ORDER BY rowid")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<(String, String, Vec<u8>)>, _>>()?;
        Ok(rows)
    })?;
    rows.into_iter()
        .map(|(id, body, sha)| {
            let id = DisplayId::parse(&id)
                .map_err(|e| VerifyError::Internal(format!("message id {id}: {e}")))?;
            Ok((id, body, sha))
        })
        .collect()
}

fn hash_of(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn copies(ctx: &Ctx<'_>, team_dir: &Path) -> Result<Vec<Mirror>, VerifyError> {
    let mut out = Vec::new();
    for (id, body, sha) in stored_messages(ctx)? {
        let Ok(sha256) = <[u8; 32]>::try_from(sha.as_slice()) else {
            continue;
        };
        // The store's copy is the source only while it still matches its hash.
        let intact = hash_of(body.as_bytes()) == sha256;
        out.push(Mirror {
            path: team_dir.join("comms").join(format!("{id}.md")),
            sha256,
            bytes: intact.then(|| body.into_bytes()),
            rewrite_differing: true,
        });
    }
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
        Ok(bytes) if hash_of(&bytes) == copy.sha256 => None,
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
    // A damaged file may fail the check outright. Its own words are the report.
    let integrity = match ctx.store.integrity_check() {
        Ok(text) => text,
        Err(StoreError::Corrupt(text)) => {
            return Ok(DeepReport {
                integrity: text,
                bad_hashes: Vec::new(),
            });
        }
        Err(e) => return Err(e.into()),
    };
    if integrity != "ok" {
        // The rows cannot be trusted, and reading them may fail. The check's
        // own output is the report.
        return Ok(DeepReport {
            integrity,
            bad_hashes: Vec::new(),
        });
    }
    let bad_hashes = stored_messages(ctx)?
        .into_iter()
        .filter(|(_, body, sha)| hash_of(body.as_bytes()).as_slice() != sha.as_slice())
        .map(|(id, _, _)| id)
        .collect();
    Ok(DeepReport {
        integrity,
        bad_hashes,
    })
}
