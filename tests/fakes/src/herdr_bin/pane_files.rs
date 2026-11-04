//! What `fake-herdr` reads from a pane directory and the session file, and
//! how it waits for the agent to react.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::state::AgentRec;
use super::{CliError, fail};
use crate::fsutil;

/// The states `status.json` holds, as `fake-claude` writes them.
pub(super) const SETTLED: [&str; 3] = ["idle", "done", "blocked"];

/// An agent the fake started is alive while its process is; one it did not
/// start (`fixed`) always is.
pub(super) fn alive(agent: &AgentRec) -> bool {
    agent.fixed || fsutil::pid_alive(agent.pid)
}

pub(super) fn env_millis(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The agent's status and its change counter.
pub(super) fn read_status(dir: &Path) -> (String, u64) {
    let doc: Option<Value> =
        fsutil::read_text(&dir.join("status.json")).and_then(|t| serde_json::from_str(&t).ok());
    let status = doc
        .as_ref()
        .and_then(|d| d.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let seq = doc
        .as_ref()
        .and_then(|d| d.get("seq"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (status, seq)
}

/// How many prompts the agent has taken from its inbox, as `fake-claude`
/// records them with each status. `None` when the status file does not say.
pub(super) fn prompts_taken(dir: &Path) -> Option<usize> {
    let doc: Value = serde_json::from_str(&fsutil::read_text(&dir.join("status.json"))?).ok()?;
    usize::try_from(doc.get("prompts")?.as_u64()?).ok()
}

pub(super) fn claude_dir() -> Option<PathBuf> {
    ["TEAMWIRE_CLAUDE_DIR", "FAKE_CLAUDE_DIR"]
        .iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
}

/// The session the agent reports: its session file if it wrote one, else the
/// id on its command line.
pub(super) fn session_of(agent: &AgentRec) -> Option<String> {
    let from_file = claude_dir()
        .map(|d| d.join("sessions").join(format!("{}.json", agent.pid)))
        .and_then(|p| fsutil::read_text(&p))
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|d| {
            d.get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    from_file.or_else(|| agent.session_hint.clone())
}

/// Waits for activity after a prompt, then for the agent to settle.
pub(super) fn wait_for_turn(
    dir: &Path,
    before_seq: u64,
    timeout_ms: Option<u64>,
) -> Result<(), CliError> {
    let started = fsutil::Stopwatch::start();
    let overall = timeout_ms.map(Duration::from_millis);
    let activity_limit = Duration::from_millis(env_millis("FAKE_HERDR_WAIT_MS", 5_000));
    let expired = |limit: Option<Duration>| limit.is_some_and(|l| started.elapsed() > l);
    let mut seen = false;
    loop {
        let (status, seq) = read_status(dir);
        if seq > before_seq {
            seen = true;
        }
        if seen && SETTLED.contains(&status.as_str()) {
            return Ok(());
        }
        if expired(overall) {
            return Err(fail("timeout", "the wait ran past the caller's timeout"));
        }
        if !seen && expired(Some(activity_limit)) {
            return Err(fail(
                "agent_prompt_stalled",
                "no working or blocked activity within five seconds",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
