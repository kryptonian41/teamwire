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
//! quit or moved to another session).

use std::io::{BufRead, BufReader, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use teamwire_core::{
    Fill, Liveness, Pid, PlaceState, Platform, SessionId, Timestamp, platform::ProcessRef,
};

use crate::extension::{FILL_ENTRY, STATE_SUFFIX};

/// The most of a session file's end that is read for the fill. The file
/// grows without bound, and a fill entry closes every turn.
const TAIL_BYTES: u64 = 4 << 20;

/// An id that is also a part of a file name must not leave its directory.
fn is_plain(id: &SessionId) -> bool {
    let s = id.as_str();
    !s.contains(['/', '\\']) && !s.starts_with('.')
}

/// The file of `session`, if pi has written one. Names start with the time,
/// so the last in order is the newest.
pub(crate) fn session_file(sessions_dir: &Path, session: &SessionId) -> Option<PathBuf> {
    if !is_plain(session) {
        return None;
    }
    let suffix = format!("_{}.jsonl", session.as_str());
    let entries = std::fs::read_dir(sessions_dir).ok()?;
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .max()
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
}

fn recorded(sessions_dir: &Path, session: &SessionId) -> Option<Recorded> {
    if !is_plain(session) {
        return None;
    }
    let file = sessions_dir.join(format!("{}{STATE_SUFFIX}", session.as_str()));
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
    Some(Recorded {
        process: ProcessRef {
            pid: Pid::new(pid),
            started,
        },
        state,
    })
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
        }) if platform.process_alive(process) => Liveness::Alive(state),
        Some(_) => Liveness::Dead,
        None => Liveness::Unknown,
    }
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
