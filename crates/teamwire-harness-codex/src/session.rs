//! Sessions: who the caller is, where a session runs, and whether it is
//! alive. All read only, from `CODEX_HOME`.
//!
//! A session's rollout is `<codex_home>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`.
//! Its first line is `{"timestamp":..,"type":"session_meta","payload":{..}}`,
//! and the payload holds `id`, `cwd` and `source` (`recorder.rs`,
//! `protocol.rs`, commit `8c07694`). While a Codex writer runs, it holds an
//! OS lock on `<codex_home>/thread-writer-locks/<id>.lock`.

use std::io::{BufRead, BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    CallerSession, Command, HarnessError, HarnessErrorCode, Liveness, Pid, PlaceState, Platform,
    SessionId, SessionKind,
};

/// The variables that name the caller's session, in the order they are read.
/// The session id comes before the thread id, because a subagent has its own
/// thread id and the root's session id. The `MCP_META_` pair is filled from
/// an MCP request's `_meta` (ADR 0023 point 8).
const SESSION_VARS: [&str; 4] = [
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
    "MCP_META_sessionId",
    "MCP_META_threadId",
];

/// The most of a rollout's first line that is read. The line holds the
/// session's metadata and can be long, but not without bound.
const FIRST_LINE_BYTES: u64 = 1 << 20;

/// An id that is also a path part must not leave its directory.
fn is_plain(id: &SessionId) -> bool {
    let s = id.as_str();
    !s.contains(['/', '\\']) && !s.starts_with('.')
}

fn dirs_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Newest first: a session being asked about is nearly always recent.
    dirs.sort_by(|a, b| b.cmp(a));
    dirs
}

/// The rollout file of `session`, if Codex has written one.
pub(crate) fn rollout_path(codex_home: &Path, session: &SessionId) -> Option<PathBuf> {
    if !is_plain(session) {
        return None;
    }
    let suffix = format!("-{}.jsonl", session.as_str());
    dirs_in(&codex_home.join("sessions"))
        .iter()
        .flat_map(|year| dirs_in(year))
        .flat_map(|month| dirs_in(&month))
        .flat_map(|day| dirs_in_files(&day))
        .find(|file| {
            file.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix))
        })
}

fn dirs_in_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries.filter_map(Result::ok).map(|e| e.path()).collect()
}

/// The `payload` of the rollout's first line, when that line is `session_meta`.
fn session_meta(rollout: &Path) -> Option<Value> {
    let file = std::fs::File::open(rollout).ok()?;
    let mut line = String::new();
    BufReader::new(file.take(FIRST_LINE_BYTES))
        .read_line(&mut line)
        .ok()?;
    let doc: Value = serde_json::from_str(&line).ok()?;
    (doc.get("type")?.as_str()? == "session_meta")
        .then(|| doc.get("payload").cloned())
        .flatten()
}

/// The folder `session` runs in, from its rollout's `session_meta`.
pub(crate) fn session_cwd(codex_home: &Path, session: &SessionId) -> Option<PathBuf> {
    let meta = session_meta(&rollout_path(codex_home, session)?)?;
    meta.get("cwd")?.as_str().map(PathBuf::from)
}

/// The caller's own session. The lookup is a parameter so a test never has
/// to change the process's own environment. The kind is `Interactive` only
/// when the rollout says `source: "cli"`, the terminal TUI. Anything else,
/// or no rollout, is `Background`, so it cannot prove a lead.
pub(crate) fn caller_session(
    codex_home: &Path,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<CallerSession> {
    let id = SESSION_VARS
        .iter()
        .find_map(|name| get(name).filter(|v| !v.trim().is_empty()))?;
    let id = SessionId::parse(id.trim()).ok()?;
    let cli = rollout_path(codex_home, &id)
        .and_then(|p| session_meta(&p))
        .is_some_and(|meta| meta.get("source").and_then(Value::as_str) == Some("cli"));
    let kind = if cli {
        SessionKind::Interactive
    } else {
        SessionKind::Background
    };
    Some(CallerSession { id, kind })
}

/// The pids that hold the session's writer lock and still run. `lsof -t`
/// exits 1 with no output when nobody holds the file.
pub(crate) fn processes(
    platform: &dyn Platform,
    codex_home: &Path,
    session: &SessionId,
    timeout: Duration,
) -> Result<Vec<Pid>, HarnessError> {
    let lock = codex_home
        .join("thread-writer-locks")
        .join(format!("{}.lock", session.as_str()));
    if !is_plain(session) || !lock.exists() {
        return Ok(Vec::new());
    }
    let cmd = Command::new("lsof")
        .arg("-t")
        .arg(lock.display().to_string());
    let out = platform
        .run(&cmd, timeout)
        .map_err(|e| HarnessError::new(HarnessErrorCode::Other, e.to_string()))?;
    if out.timed_out {
        return Err(HarnessError::new(
            HarnessErrorCode::Timeout,
            "`lsof` ran past its bound",
        ));
    }
    if out.exit_code != 0 && !(out.exit_code == 1 && out.stdout.is_empty()) {
        return Err(HarnessError::new(
            HarnessErrorCode::Other,
            format!("`lsof` exited {}", out.exit_code),
        ));
    }
    let mut pids: Vec<Pid> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .map(Pid::new)
        .filter(|p| platform.pid_alive(*p))
        .collect();
    pids.sort();
    pids.dedup();
    Ok(pids)
}

/// `Alive` when a process holds the lock. `Dead` when the rollout exists and
/// nothing holds the lock. Otherwise `Unknown`, which includes a failed
/// `lsof`: the session may be running with only the listing broken.
pub(crate) fn liveness(
    platform: &dyn Platform,
    codex_home: &Path,
    session: &SessionId,
    timeout: Duration,
) -> Liveness {
    match processes(platform, codex_home, session, timeout) {
        Ok(pids) if !pids.is_empty() => Liveness::Alive(PlaceState::Unknown),
        Ok(_) if rollout_path(codex_home, session).is_some() => Liveness::Dead,
        _ => Liveness::Unknown,
    }
}
