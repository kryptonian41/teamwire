//! The parts of herdr's results this adapter reads, shaped by
//! `herdr api schema --json` (protocol 22) and real output. Every field is
//! optional unless the adapter cannot work without it: an unnamed agent (the
//! user's own session) must not make a whole list fail to parse.

use serde::Deserialize;
use serde_json::Value;
use teamwire_core::{HostError, HostErrorCode, PlaceState};

#[derive(Debug, Deserialize)]
pub(crate) struct AgentSession {
    #[serde(default)]
    pub value: Option<String>,
}

/// `AgentInfo`.
#[derive(Debug, Deserialize)]
pub(crate) struct AgentInfo {
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent_status: Option<String>,
    #[serde(default)]
    pub agent_session: Option<AgentSession>,
}

impl AgentInfo {
    /// `idle` and `done` both mean ready for input (`herdr --skill`).
    /// `interactive_ready` is a launch flag and plays no part here.
    pub(crate) fn place_state(&self) -> PlaceState {
        match self.agent_status.as_deref() {
            Some("working") => PlaceState::Working,
            Some("idle" | "done") => PlaceState::Idle,
            Some("blocked") => PlaceState::Blocked,
            _ => PlaceState::Unknown,
        }
    }

    /// The workspace the agent's pane is in: its own field, else the part
    /// of the pane id before the colon (`w44:p6`).
    pub(crate) fn workspace(&self) -> Option<&str> {
        self.workspace_id
            .as_deref()
            .or_else(|| self.pane_id.as_deref()?.split_once(':').map(|(w, _)| w))
    }

    pub(crate) fn session_value(&self) -> Option<&str> {
        self.agent_session
            .as_ref()
            .and_then(|s| s.value.as_deref())
            .filter(|v| !v.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct AgentEnvelope {
    agent: AgentInfo,
}

#[derive(Debug, Deserialize)]
struct AgentListEnvelope {
    #[serde(default)]
    agents: Vec<AgentInfo>,
}

#[derive(Debug, Deserialize)]
struct Read {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct ReadEnvelope {
    read: Read,
}

#[derive(Debug, Deserialize)]
struct TabRef {
    tab_id: String,
}

#[derive(Debug, Deserialize)]
struct PaneRef {
    pane_id: String,
}

/// `workspace_created` and `tab_created`: the tab and its first pane.
#[derive(Debug, Deserialize)]
struct Created {
    tab: TabRef,
    root_pane: PaneRef,
}

/// Where a new tab's first pane is.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NewTab {
    pub tab: String,
    pub pane: String,
}

fn parse<T: serde::de::DeserializeOwned>(what: &str, result: Value) -> Result<T, HostError> {
    serde_json::from_value(result).map_err(|e| {
        HostError::new(
            HostErrorCode::Other,
            format!("herdr {what} result has an unexpected shape: {e}"),
        )
    })
}

pub(crate) fn agent_info(result: Value) -> Result<AgentInfo, HostError> {
    parse::<AgentEnvelope>("agent", result).map(|e| e.agent)
}

pub(crate) fn agent_list(result: Value) -> Result<Vec<AgentInfo>, HostError> {
    parse::<AgentListEnvelope>("agent list", result).map(|e| e.agents)
}

/// `.result.read.text` of a `pane_read` result.
pub(crate) fn read_text(result: Value) -> Result<String, HostError> {
    parse::<ReadEnvelope>("read", result).map(|e| e.read.text)
}

pub(crate) fn new_tab(result: Value) -> Result<NewTab, HostError> {
    let c: Created = parse("workspace or tab create", result)?;
    Ok(NewTab {
        tab: c.tab.tab_id,
        pane: c.root_pane.pane_id,
    })
}
