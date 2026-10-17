//! The folder a session runs in. `claude --resume` only finds a session from
//! that session's own folder. Read only: the live session file first, then
//! the session's transcript. Neither is ever written.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;
use teamwire_core::SessionId;

use crate::session::read_session_files;

/// How many transcript lines are read looking for a `cwd`. The first lines of
/// a transcript carry it; a bound keeps a huge file from being read whole.
const TRANSCRIPT_LINES: usize = 200;

/// The `cwd` of the session file naming `session`, else the first `cwd` in
/// the session's transcript `<claude_dir>/projects/*/<session>.jsonl`.
pub(crate) fn session_cwd(claude_dir: &Path, session: &SessionId) -> Option<PathBuf> {
    read_session_files(claude_dir)
        .into_iter()
        .find(|f| f.session_id == session.as_str())
        .and_then(|f| f.cwd)
        .or_else(|| transcript_cwd(claude_dir, session))
}

fn transcript_cwd(claude_dir: &Path, session: &SessionId) -> Option<PathBuf> {
    let projects = std::fs::read_dir(claude_dir.join("projects")).ok()?;
    let mut dirs: Vec<PathBuf> = projects.filter_map(Result::ok).map(|e| e.path()).collect();
    dirs.sort();
    dirs.into_iter()
        .map(|d| d.join(format!("{}.jsonl", session.as_str())))
        .find_map(|file| first_cwd(&file))
}

fn first_cwd(file: &Path) -> Option<PathBuf> {
    let reader = BufReader::new(std::fs::File::open(file).ok()?);
    reader
        .lines()
        .take(TRANSCRIPT_LINES)
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find_map(|doc| doc.get("cwd")?.as_str().map(PathBuf::from))
}
