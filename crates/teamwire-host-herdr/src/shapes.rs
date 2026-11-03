//! The parts of herdr's results this adapter reads, shaped by
//! `herdr api schema --json` (protocol 22) and real output. Every field is
//! optional unless the adapter cannot work without it: an unnamed agent (the
//! user's own session) must not make a whole list fail to parse.

use serde::Deserialize;
use serde_json::Value;
use teamwire_core::{HostError, HostErrorCode, PlaceState};

/// Who runs in a pane, as herdr's agent integration reported it: `kind` is
/// `id` for a session id, or `path` for a session file (pi).
#[derive(Debug, Deserialize)]
pub(crate) struct AgentSession {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
}

/// The session id in the name of a pi session file,
/// `<time>_<id>.jsonl`.
fn pi_session_of_file(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?.strip_suffix(".jsonl")?;
    let (_, id) = name.rsplit_once('_')?;
    (!id.is_empty()).then_some(id)
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

    /// The session the pane runs. herdr's pi integration reports pi's
    /// session file rather than its id, so the id is read from the file's
    /// name. Any other report is passed on as herdr gave it.
    pub(crate) fn session_value(&self) -> Option<&str> {
        let session = self.agent_session.as_ref()?;
        let value = session.value.as_deref().filter(|v| !v.is_empty())?;
        match (session.agent.as_deref(), session.kind.as_deref()) {
            (Some("pi"), Some("path")) => pi_session_of_file(value),
            _ => Some(value),
        }
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

pub(crate) fn new_tab(result: Value) -> Result<NewTab, HostError> {
    let c: Created = parse("workspace or tab create", result)?;
    Ok(NewTab {
        tab: c.tab.tab_id,
        pane: c.root_pane.pane_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(session: &str) -> AgentInfo {
        serde_json::from_str(&format!(r#"{{"agent_session": {session}}}"#)).unwrap()
    }

    #[test]
    fn a_session_reported_by_id_is_that_id() {
        let info = agent(
            r#"{"agent": "claude", "kind": "id", "source": "herdr:claude", "value": "46e0fa45-9b35-4ded-b055-b86c0b68cbc4"}"#,
        );
        assert_eq!(
            info.session_value(),
            Some("46e0fa45-9b35-4ded-b055-b86c0b68cbc4")
        );
    }

    /// herdr 0.9.5's pi integration reports the session file's path, which
    /// pi names `<time>_<id>.jsonl`, before the file exists too.
    #[test]
    fn a_pi_session_reported_by_path_is_the_id_in_its_file_name() {
        let info = agent(
            r#"{"agent": "pi", "kind": "path", "source": "herdr:pi", "value": "/Users/u/.pi/agent/sessions/teamwire/2026-10-29T11-58-32-273Z_b3f94d59-ffc0-485e-b113-950e10f3429a.jsonl"}"#,
        );
        assert_eq!(
            info.session_value(),
            Some("b3f94d59-ffc0-485e-b113-950e10f3429a")
        );
    }

    #[test]
    fn a_pi_path_without_pis_file_name_names_no_session() {
        for path in [
            "/x/no-underscore.jsonl",
            "/x/2026_abc.txt",
            "/x/2026_.jsonl",
            "",
        ] {
            let info = agent(&format!(
                r#"{{"agent": "pi", "kind": "path", "value": "{path}"}}"#
            ));
            assert_eq!(info.session_value(), None, "{path}");
        }
    }

    #[test]
    fn another_agents_path_is_passed_on_as_it_was() {
        let info = agent(r#"{"agent": "codex", "kind": "path", "value": "/x/rollout-1.jsonl"}"#);
        assert_eq!(info.session_value(), Some("/x/rollout-1.jsonl"));
    }
}
