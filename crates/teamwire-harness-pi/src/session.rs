//! Sessions: where one runs, whether it is alive, and how full its context
//! is. All read only, from the folder pi writes member sessions to.
//!
//! A session file is `<sessions_dir>/<time>_<id>.jsonl`. Its first line is
//! `{"type":"session","id":..,"cwd":..}` (pi's `session-format.md`). pi
//! writes the file at the session's first message, with every entry made
//! before it. At each turn's end the member's extension adds a `custom`
//! entry `teamwire.fill`, pi's own context reading, which pi keeps out of
//! the model's context.
//!
//! The extension also writes `<sessions_dir>/<id>.teamwire.json` from the
//! moment pi starts: `{"pid", "started_at_ms", "state"}`, where `state` is
//! `idle`, `working`, `blocked` (an extension's dialog waits) or `ended` (pi
//! quit or moved to another session). While a member's call runs, the file
//! also holds `tool: {name, started_at_ms}`, the newest call with no result
//! yet. The lead's extension writes the same file for the lead's session,
//! with `"lead": true`, pi's `mode` (`tui` for the terminal UI) and
//! `session_file`, because a lead's session file lives in pi's own folder
//! for its cwd, not in `<sessions_dir>`.

use std::io::{BufRead, BufReader, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use teamwire_core::{
    CallerSession, Fill, Liveness, Pid, PlaceState, Platform, SessionId, SessionKind, SessionUsage,
    Timestamp, TokenCounts, ToolRun, platform::ProcessRef,
};

use crate::extension::{FILL_ENTRY, STATE_SUFFIX};

/// The most of a session file's end that is read for the fill. The file
/// grows without bound, and a fill entry closes every turn.
const TAIL_BYTES: u64 = 4 << 20;

/// A session file larger than this is not read for usage: the turn-end hook
/// has a time budget, and usage needs the whole file.
const MAX_USAGE_BYTES: u64 = 64 << 20;

/// An id that is also a part of a file name must not leave its directory.
fn is_plain(id: &SessionId) -> bool {
    let s = id.as_str();
    !s.contains(['/', '\\']) && !s.starts_with('.')
}

/// The file of `session`, if pi has written one: a member's in
/// `sessions_dir`, where names start with the time, so the last in order is
/// the newest; else the one a lead's state file names.
pub(crate) fn session_file(sessions_dir: &Path, session: &SessionId) -> Option<PathBuf> {
    if !is_plain(session) {
        return None;
    }
    let suffix = format!("_{}.jsonl", session.as_str());
    let member = std::fs::read_dir(sessions_dir).ok().and_then(|entries| {
        entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(&suffix))
            })
            .max()
    });
    member.or_else(|| {
        recorded(sessions_dir, session)?
            .session_file
            .filter(|f| f.is_file())
    })
}

fn lines(file: &Path) -> impl Iterator<Item = String> {
    std::fs::File::open(file)
        .ok()
        .map(|f| BufReader::new(f).lines().map_while(Result::ok))
        .into_iter()
        .flatten()
}

/// The folder `session` runs in, from its file's header.
pub(crate) fn session_cwd(sessions_dir: &Path, session: &SessionId) -> Option<PathBuf> {
    let first = lines(&session_file(sessions_dir, session)?).next()?;
    let doc: Value = serde_json::from_str(&first).ok()?;
    (doc.get("type")?.as_str()? == "session")
        .then(|| doc.get("cwd")?.as_str().map(PathBuf::from))
        .flatten()
}

/// The `data` of a line that is a `custom` entry of `kind`.
fn custom_data(line: &str, kind: &str) -> Option<Value> {
    let doc: Value = serde_json::from_str(line).ok()?;
    (doc.get("type")?.as_str()? == "custom" && doc.get("customType")?.as_str()? == kind)
        .then(|| doc.get("data").cloned())
        .flatten()
}

/// What a session's state file says.
struct Recorded {
    process: ProcessRef,
    /// `None` once pi no longer runs the session.
    state: Option<PlaceState>,
    /// Set only by the lead's extension.
    lead: bool,
    /// pi's mode; `tui` is the terminal UI.
    mode: Option<String>,
    session_file: Option<PathBuf>,
    /// The call that runs now, as the member's extension stamped it.
    tool: Option<ToolRun>,
}

fn recorded(sessions_dir: &Path, session: &SessionId) -> Option<Recorded> {
    if !is_plain(session) {
        return None;
    }
    read_recorded(&sessions_dir.join(format!("{}{STATE_SUFFIX}", session.as_str())))
}

fn read_recorded(file: &Path) -> Option<Recorded> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let pid = u32::try_from(doc.get("pid")?.as_u64()?).ok()?;
    let started = doc
        .get("started_at_ms")
        .and_then(Value::as_i64)
        .and_then(Timestamp::from_unix_millis);
    let state = match doc.get("state").and_then(Value::as_str) {
        Some("ended") => None,
        Some("idle") => Some(PlaceState::Idle),
        Some("working") => Some(PlaceState::Working),
        Some("blocked") => Some(PlaceState::Blocked),
        _ => Some(PlaceState::Unknown),
    };
    let text = |key: &str| doc.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(Recorded {
        process: ProcessRef {
            pid: Pid::new(pid),
            started,
        },
        state,
        lead: doc.get("lead").and_then(Value::as_bool) == Some(true),
        mode: text("mode"),
        session_file: text("session_file").map(PathBuf::from),
        tool: doc.get("tool").and_then(|t| {
            Some(ToolRun {
                name: t.get("name")?.as_str()?.to_owned(),
                started_at: Timestamp::from_unix_millis(t.get("started_at_ms")?.as_i64()?)?,
            })
        }),
    })
}

/// A lead's session from its state file, while the recorded pi process
/// still runs it. Only pi's terminal UI is an interactive session.
fn lead_session(id: SessionId, r: &Recorded, platform: &dyn Platform) -> Option<CallerSession> {
    if !r.lead || r.state.is_none() || !platform.process_alive(r.process) {
        return None;
    }
    let kind = if r.mode.as_deref() == Some("tui") {
        SessionKind::Interactive
    } else {
        SessionKind::Background
    };
    Some(CallerSession { id, kind })
}

/// The lead session `session`, when the lead's extension runs it, and the
/// pi process that runs it.
pub(crate) fn lead_by_id(
    platform: &dyn Platform,
    sessions_dir: &Path,
    session: &SessionId,
) -> Option<(CallerSession, Pid)> {
    let r = recorded(sessions_dir, session)?;
    lead_session(session.clone(), &r, platform).map(|s| (s, r.process.pid))
}

/// The pi process that runs `session`, when it still does: the recorded pid,
/// alive, started when the record says, and not moved off the session.
pub(crate) fn processes(
    platform: &dyn Platform,
    sessions_dir: &Path,
    session: &SessionId,
) -> Vec<Pid> {
    match recorded(sessions_dir, session) {
        Some(r) if r.state.is_some() && platform.process_alive(r.process) => vec![r.process.pid],
        _ => Vec::new(),
    }
}

/// `Alive` with the recorded state while the recorded process runs the
/// session. `Dead` when the process ended or left the session. `Unknown`
/// with no state file: a session started before pi loaded the extension.
pub(crate) fn liveness(
    platform: &dyn Platform,
    sessions_dir: &Path,
    session: &SessionId,
) -> Liveness {
    match recorded(sessions_dir, session) {
        Some(Recorded {
            process,
            state: Some(state),
            ..
        }) if platform.process_alive(process) => Liveness::Alive(state),
        Some(_) => Liveness::Dead,
        None => Liveness::Unknown,
    }
}

/// The call `session` runs now, while the recorded process still runs the
/// session: a dead process stamps no result, so its `tool` is not believed.
pub(crate) fn current_tool(
    platform: &dyn Platform,
    sessions_dir: &Path,
    session: &SessionId,
) -> Option<ToolRun> {
    recorded(sessions_dir, session)
        .filter(|r| r.state.is_some() && platform.process_alive(r.process))
        .and_then(|r| r.tool)
}

/// The fill from the newest `teamwire.fill` entry in the end of the file.
/// pi's reading counts what the next request will send, cached tokens
/// included, against the model's window, as the Claude figure does.
pub(crate) fn context_fill(file: &Path) -> Option<Fill> {
    let tail = tail_text(file)?;
    tail.lines().rev().find_map(|line| {
        let data = custom_data(line, FILL_ENTRY)?;
        Some(Fill {
            tokens: data.get("tokens")?.as_u64()?,
            window: data.get("window")?.as_u64()?,
        })
    })
}

/// The token usage of every assistant message in the file, by model. A line
/// is `{"type":"message","id":..,"message":{"role":"assistant","model":..,
/// "usage":{"input","output","cacheRead","cacheWrite",..}}}` (pi's
/// `session-format.md`). pi writes each response once, as one entry with
/// its own id, so each line counts once. `None` when the file is missing,
/// larger than `MAX_USAGE_BYTES`, or holds no assistant message.
pub(crate) fn session_usage(file: &Path) -> Option<SessionUsage> {
    std::fs::metadata(file)
        .ok()
        .filter(|m| m.is_file() && m.len() <= MAX_USAGE_BYTES)?;
    let mut totals = SessionUsage::new();
    for line in lines(file) {
        let Ok(doc) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(message) = doc
            .get("message")
            .filter(|_| doc.get("type").and_then(Value::as_str) == Some("message"))
            .filter(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
        else {
            continue;
        };
        let (Some(model), Some(usage)) = (
            message.get("model").and_then(Value::as_str),
            message.get("usage"),
        ) else {
            continue;
        };
        let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
        totals
            .entry(model.to_owned())
            .or_default()
            .add(TokenCounts {
                input: count("input"),
                output: count("output"),
                cache_read: count("cacheRead"),
                cache_creation: count("cacheWrite"),
            });
    }
    (!totals.is_empty()).then_some(totals)
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
