//! The JSON file behind `fake-herdr`, its lock, and the pane directories the
//! agent process and the CLI use to talk to each other.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::fsutil;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Workspace {
    pub(crate) workspace_id: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields are herdr's own JSON keys"
)]
pub(crate) struct Tab {
    pub(crate) tab_id: String,
    pub(crate) workspace_id: String,
    pub(crate) label: String,
}

/// An agent started in a pane.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct AgentRec {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) pid: u32,
    /// The session id named on the agent's command line, if any. The
    /// session file the agent writes wins when it exists.
    pub(crate) session_hint: Option<String>,
    /// An agent the fake did not start (see `add-unnamed-agent`): no process,
    /// always alive, and its `name` is empty, so it has none in the JSON.
    #[serde(default)]
    pub(crate) fixed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields are herdr's own JSON keys"
)]
pub(crate) struct Pane {
    pub(crate) pane_id: String,
    pub(crate) tab_id: String,
    pub(crate) workspace_id: String,
    pub(crate) cwd: String,
    #[serde(default)]
    pub(crate) label: Option<String>,
    pub(crate) agent: Option<AgentRec>,
    /// The `--env` pairs of the `pane split` that made the pane. `pane run`
    /// hands them to the program it starts.
    #[serde(default)]
    pub(crate) env: Vec<(String, String)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct State {
    pub(crate) counter: u64,
    pub(crate) workspaces: Vec<Workspace>,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) panes: Vec<Pane>,
    /// How many times each fault rule has fired, by rule index.
    pub(crate) fault_hits: BTreeMap<String, u32>,
}

impl State {
    pub(crate) fn next_id(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    pub(crate) fn pane(&self, id: &str) -> Option<&Pane> {
        self.panes.iter().find(|p| p.pane_id == id)
    }

    /// The pane holding the agent named `target`, or the pane with that id.
    pub(crate) fn find_agent_pane(&self, target: &str) -> Option<&Pane> {
        self.panes
            .iter()
            .find(|p| p.pane_id == target || p.agent.as_ref().is_some_and(|a| a.name == target))
    }
}

/// Where a pane keeps its screen, status, inbox and key queue.
pub(crate) fn pane_dir(state_path: &Path, pane_id: &str) -> PathBuf {
    let dir = state_path.parent().unwrap_or_else(|| Path::new("."));
    fsutil::join(dir, &["fake-herdr-panes", &pane_id.replace(':', "_")])
}

/// Holds the state lock until dropped. The lock is the kernel's, as in
/// `owner.rs`: it ends with the open file, so a call that is killed never
/// leaves it held.
#[derive(Debug)]
pub(crate) struct Lock {
    _file: File,
}

/// Takes the lock on the file beside the state file, waiting up to five
/// seconds. The file stays on disk; only the lock on it comes and goes.
#[expect(
    clippy::disallowed_methods,
    reason = "the fake state lock opens a file that is never replaced"
)]
pub(crate) fn lock(state_path: &Path) -> Result<Lock, String> {
    let lock_path = state_path.with_extension("lock");
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| e.to_string())?;
    let start = fsutil::Stopwatch::start();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Lock { _file: file }),
            Err(TryLockError::WouldBlock) => {
                if start.elapsed() > Duration::from_secs(5) {
                    return Err(format!("state lock {} stayed held", lock_path.display()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryLockError::Error(e)) => return Err(e.to_string()),
        }
    }
}

pub(crate) fn load(state_path: &Path) -> Result<State, String> {
    match fsutil::read_text(state_path) {
        None => Ok(State::default()),
        Some(text) if text.trim().is_empty() => Ok(State::default()),
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("bad state file: {e}")),
    }
}

pub(crate) fn save(state_path: &Path, state: &State) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    fsutil::write_atomic(state_path, &bytes).map_err(|e| e.to_string())
}
