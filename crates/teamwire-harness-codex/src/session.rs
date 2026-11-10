//! Sessions: who the caller is, where a session runs, and whether it is
//! alive. All read only, from `CODEX_HOME`.
//!
//! A session's rollout is `<codex_home>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`.
//! Its first line is `{"timestamp":..,"type":"session_meta","payload":{..}}`,
//! and the payload holds `id`, `cwd` and `source` (`recorder.rs`,
//! `protocol.rs`, commit `8c07694`). While a Codex writer runs, it holds an
//! OS lock on `<codex_home>/thread-writer-locks/<id>.lock`.

use std::io::{BufRead, BufReader, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    CallerSession, Command, Fill, HarnessError, HarnessErrorCode, Liveness, Pid, PlaceState,
    Platform, SessionId, SessionKind, SessionUsage, TokenCounts,
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

/// The most of a rollout's end that is read for the fill. A rollout grows
/// without bound, and the newest `token_count` event is near its end.
const TAIL_BYTES: u64 = 4 << 20;

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

/// The fill of `session`, from the newest `token_count` event in the end of
/// its rollout. An event line is
/// `{"type":"event_msg","payload":{"type":"token_count","info":{..}}}`, and
/// `info` is `TokenUsageInfo` (openai/codex `codex-rs/protocol/src/protocol.rs`):
/// `last_token_usage.input_tokens` is what the model read on the last request,
/// cached tokens included, and `model_context_window` is the window. Codex's
/// own "context left" subtracts a 12,000-token baseline from both; this does
/// not, so the figure means the same as the Claude one. No live Codex run
/// checked this: Codex is not installed where it was written, so the shape
/// comes from that source file alone.
pub(crate) fn context_fill(codex_home: &Path, session: &SessionId) -> Option<Fill> {
    let tail = tail_text(&rollout_path(codex_home, session)?)?;
    tail.lines().rev().find_map(|line| {
        let doc: Value = serde_json::from_str(line).ok()?;
        let payload = doc.get("payload")?;
        if payload.get("type")?.as_str()? != "token_count" {
            return None;
        }
        let info = payload.get("info").filter(|i| !i.is_null())?;
        let tokens = info
            .get("last_token_usage")?
            .get("input_tokens")?
            .as_u64()?;
        let window = info.get("model_context_window")?.as_u64()?;
        Some(Fill { tokens, window })
    })
}

/// The session's usage, from the newest `token_count` event with `info` in
/// the end of `file`: `info.total_token_usage` holds the session's running
/// totals (`TokenUsage` in openai/codex `codex-rs/protocol/src/protocol.rs`).
/// Its `input_tokens` holds the cached tokens, so `cached_input_tokens` is
/// taken out of it and counted as cache reads. `output_tokens` holds the
/// reasoning tokens. Codex reports no cache writes, so that count is 0. The
/// model is the newest `turn_context` line's `payload.model` before that
/// event. As for the fill, no live Codex run checked this shape.
///
/// shortcut: a session that changed model has all its tokens counted under
/// the newest one; sum the change between events per model if members ever
/// switch models mid-session.
pub(crate) fn session_usage(file: &Path) -> Option<SessionUsage> {
    let tail = tail_text(file)?;
    let mut lines = tail
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok());
    let total = lines.by_ref().find_map(|doc| {
        let payload = doc.get("payload")?;
        (payload.get("type")?.as_str()? == "token_count")
            .then(|| payload.get("info")?.get("total_token_usage").cloned())
            .flatten()
    })?;
    let model = lines
        .find_map(|doc| {
            (doc.get("type")?.as_str()? == "turn_context")
                .then(|| {
                    doc.get("payload")?
                        .get("model")?
                        .as_str()
                        .map(str::to_owned)
                })
                .flatten()
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let count = |key: &str| total.get(key).and_then(Value::as_u64).unwrap_or(0);
    let cached = count("cached_input_tokens");
    let counts = TokenCounts {
        input: count("input_tokens").saturating_sub(cached),
        output: count("output_tokens"),
        cache_read: cached,
        cache_creation: 0,
    };
    Some(SessionUsage::from([(model, counts)]))
}

/// The last `TAIL_BYTES` of the file, without its first line when the cut
/// falls inside it.
fn tail_text(file: &Path) -> Option<String> {
    let mut f = std::fs::File::open(file).ok()?;
    let start = f.metadata().ok()?.len().saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.take(TAIL_BYTES).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start == 0 {
        return Some(text);
    }
    text.split_once('\n').map(|(_, rest)| rest.to_owned())
}
