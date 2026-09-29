//! Sessions: who the caller is, and whether a member's session is running.
//!
//! Two read-only sources, in this order. First the session files Claude Code
//! keeps at `<claude_dir>/sessions/<pid>.json`. Then `claude agents --json`
//! for a session no live file names (a background session, or a file the
//! program has not written yet). Neither source is ever written.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    CallerSession, Command, HarnessError, HarnessErrorCode, Liveness, Pid, PlaceState, Platform,
    SessionId, SessionKind,
};

/// The most a `claude agents --json` call may take. A slower call is not
/// waited for, because the callers (watch, close) poll on a short clock.
const LISTING_BOUND: Duration = Duration::from_secs(5);

/// One `sessions/<pid>.json`, reduced to what liveness needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionFile {
    pub(crate) pid: Pid,
    pub(crate) session_id: String,
    pub(crate) status: Option<String>,
}

/// The caller's own session, from a lookup of environment variables. The
/// lookup is a parameter so a test never has to change the process's own
/// environment.
pub(crate) fn caller_from_env(get: impl Fn(&str) -> Option<String>) -> Option<CallerSession> {
    let id = get("CLAUDE_CODE_SESSION_ID")?;
    let id = SessionId::parse(id.trim()).ok()?;
    // `bg` is a session Claude Code started without a terminal. Any other
    // value, or none, is an interactive session.
    let kind = match get("CLAUDE_CODE_SESSION_KIND").as_deref().map(str::trim) {
        Some("bg") => SessionKind::Background,
        _ => SessionKind::Interactive,
    };
    Some(CallerSession { id, kind })
}

/// Every readable session file. A missing directory, an unreadable file and
/// a file that is not a session are all skipped: the directory is Claude
/// Code's, and one bad file must not hide the others.
pub(crate) fn read_session_files(claude_dir: &Path) -> Vec<SessionFile> {
    let Ok(entries) = std::fs::read_dir(claude_dir.join("sessions")) else {
        return Vec::new();
    };
    let mut files: Vec<SessionFile> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| parse_session_file(&std::fs::read(p).ok()?))
        .collect();
    files.sort_by_key(|f| f.pid);
    files
}

fn parse_session_file(bytes: &[u8]) -> Option<SessionFile> {
    let doc: Value = serde_json::from_slice(bytes).ok()?;
    let pid = u32::try_from(doc.get("pid")?.as_u64()?).ok()?;
    Some(SessionFile {
        pid: Pid::new(pid),
        session_id: doc.get("sessionId")?.as_str()?.to_owned(),
        status: doc.get("status").and_then(Value::as_str).map(str::to_owned),
    })
}

/// What a session's `status` (a file) or `state` (a background row) says.
/// Values seen in real files: `idle`, `busy`, `waiting`, `shell`. A value
/// nobody has seen is `Unknown`, not a guess.
fn place_state(status: Option<&str>) -> PlaceState {
    match status {
        Some("idle") => PlaceState::Idle,
        Some("busy" | "working" | "running" | "shell") => PlaceState::Working,
        Some("waiting" | "blocked") => PlaceState::Blocked,
        _ => PlaceState::Unknown,
    }
}

/// A background row whose job has ended is listed but is not running.
fn has_ended(state: Option<&str>) -> bool {
    matches!(
        state,
        Some("done" | "completed" | "failed" | "stopped" | "exited" | "cancelled" | "killed")
    )
}

/// One row of `claude agents --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentRow {
    pub(crate) pid: Option<Pid>,
    pub(crate) state: Option<String>,
}

/// Why a listing gave no answer.
#[derive(Debug)]
pub(crate) enum ListingFailure {
    TimedOut,
    Failed(String),
}

impl ListingFailure {
    pub(crate) fn into_error(self) -> HarnessError {
        match self {
            Self::TimedOut => HarnessError::new(
                HarnessErrorCode::Timeout,
                "`claude agents --json` ran past its bound",
            ),
            Self::Failed(why) => HarnessError::new(HarnessErrorCode::Other, why),
        }
    }
}

/// The rows of `claude agents --json` that name `session`. An interactive
/// row keeps its state under `status` and a background row under `state`.
pub(crate) fn list_rows(
    platform: &dyn Platform,
    claude_bin: &str,
    session: &SessionId,
    timeout: Duration,
) -> Result<Vec<AgentRow>, ListingFailure> {
    let cmd = Command::new(claude_bin).args(["agents", "--json"]);
    let out = platform
        .run(&cmd, timeout.min(LISTING_BOUND))
        .map_err(|e| ListingFailure::Failed(e.to_string()))?;
    if out.timed_out {
        return Err(ListingFailure::TimedOut);
    }
    if out.exit_code != 0 {
        return Err(ListingFailure::Failed(format!(
            "`claude agents --json` exited {}",
            out.exit_code
        )));
    }
    let doc: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| ListingFailure::Failed(format!("`claude agents --json` is not JSON: {e}")))?;
    let Some(rows) = doc.as_array() else {
        return Err(ListingFailure::Failed(
            "`claude agents --json` is not a list".to_owned(),
        ));
    };
    let text = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(rows
        .iter()
        .filter(|row| {
            text(row, "sessionId").as_deref() == Some(session.as_str())
                || text(row, "id").as_deref() == Some(session.as_str())
        })
        .map(|row| AgentRow {
            pid: row
                .get("pid")
                .and_then(Value::as_u64)
                .and_then(|p| u32::try_from(p).ok())
                .map(Pid::new),
            state: text(row, "status").or_else(|| text(row, "state")),
        })
        .collect())
}

/// Liveness of `session`: the session files, then the listing.
///
/// A file whose process is gone is not alive. A listing that fails is
/// `Unknown`, never `Dead`: the program may be running and only the
/// listing broken.
pub(crate) fn liveness(
    platform: &dyn Platform,
    claude_dir: &Path,
    claude_bin: &str,
    session: &SessionId,
    timeout: Duration,
) -> Liveness {
    let running = read_session_files(claude_dir)
        .into_iter()
        .find(|f| f.session_id == session.as_str() && platform.pid_alive(f.pid));
    if let Some(file) = running {
        return Liveness::Alive(place_state(file.status.as_deref()));
    }
    match list_rows(platform, claude_bin, session, timeout) {
        Err(_) => Liveness::Unknown,
        Ok(rows) => rows
            .into_iter()
            .find(|r| !has_ended(r.state.as_deref()) && r.pid.is_none_or(|p| platform.pid_alive(p)))
            .map_or(Liveness::Dead, |r| {
                Liveness::Alive(place_state(r.state.as_deref()))
            }),
    }
}

/// The running processes of `session`, from the files, else the listing.
pub(crate) fn processes(
    platform: &dyn Platform,
    claude_dir: &Path,
    claude_bin: &str,
    session: &SessionId,
    timeout: Duration,
) -> Result<Vec<Pid>, HarnessError> {
    let mut pids: Vec<Pid> = read_session_files(claude_dir)
        .into_iter()
        .filter(|f| f.session_id == session.as_str() && platform.pid_alive(f.pid))
        .map(|f| f.pid)
        .collect();
    if pids.is_empty() {
        let rows = list_rows(platform, claude_bin, session, timeout)
            .map_err(ListingFailure::into_error)?;
        pids = rows
            .into_iter()
            .filter(|r| !has_ended(r.state.as_deref()))
            .filter_map(|r| r.pid)
            .filter(|p| platform.pid_alive(*p))
            .collect();
    }
    pids.sort();
    pids.dedup();
    Ok(pids)
}
