//! herdr's info objects (`AgentInfo`, `PaneInfo`, `TabInfo`, `WorkspaceInfo`),
//! with the fields and names of `herdr api schema --json` (protocol 22).

use serde_json::{Map, Value, json};

use super::Ctx;
use super::pane_files::{alive, read_status, session_of};
use super::state::{AgentRec, Pane, Tab, Workspace, pane_dir};

/// The agent in a pane, if it is running.
pub(super) fn live_agent(pane: &Pane) -> Option<&AgentRec> {
    pane.agent.as_ref().filter(|a| alive(a))
}

/// Ready for input: real herdr keeps `interactive_ready` true while the
/// agent is idle, working or done, and false when it waits at a dialog.
fn ready(status: &str) -> bool {
    matches!(status, "idle" | "working" | "done")
}

fn insert(doc: &mut Value, key: &str, value: Value) {
    if let Some(map) = doc.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
}

fn agent_fields(agent: &AgentRec, doc: &mut Value) {
    insert(doc, "agent", json!(agent.kind));
    if let Some(session) = session_of(agent) {
        insert(
            doc,
            "agent_session",
            json!({ "agent": agent.kind, "kind": "id", "source": "herdr:claude", "value": session }),
        );
    }
}

/// The pane's `agent_status` and change counter: the agent's, or `unknown`
/// for a pane that runs no agent.
fn status_of(ctx: &Ctx<'_>, pane: &Pane) -> (String, u64) {
    match live_agent(pane) {
        Some(_) => read_status(&pane_dir(ctx.state_path, &pane.pane_id)),
        None => ("unknown".to_owned(), 0),
    }
}

pub(super) fn pane_info(ctx: &Ctx<'_>, pane: &Pane) -> Value {
    let (status, seq) = status_of(ctx, pane);
    let mut doc = json!({
        "pane_id": pane.pane_id,
        "terminal_id": format!("term_{}", pane.pane_id.replace(':', "")),
        "workspace_id": pane.workspace_id,
        "tab_id": pane.tab_id,
        "focused": false,
        "agent_status": status,
        "revision": seq,
        "cwd": pane.cwd,
        "foreground_cwd": pane.cwd,
    });
    if let Some(label) = &pane.label {
        insert(&mut doc, "label", json!(label));
    }
    if let Some(agent) = live_agent(pane) {
        agent_fields(agent, &mut doc);
    }
    doc
}

/// `AgentInfo`: the pane's fields plus the agent's name (absent for an
/// agent that has none), `state_change_seq` and `interactive_ready`.
pub(super) fn agent_info(ctx: &Ctx<'_>, pane: &Pane, agent: &AgentRec) -> Value {
    let (status, seq) = status_of(ctx, pane);
    let mut doc = pane_info(ctx, pane);
    if !agent.name.is_empty() {
        insert(&mut doc, "name", json!(agent.name));
    }
    insert(&mut doc, "interactive_ready", json!(ready(&status)));
    insert(&mut doc, "state_change_seq", json!(seq));
    doc
}

fn rollup(ctx: &Ctx<'_>, panes: &[&Pane]) -> String {
    panes
        .iter()
        .find(|p| live_agent(p).is_some())
        .map_or_else(|| "unknown".to_owned(), |p| status_of(ctx, p).0)
}

pub(super) fn tab_info(ctx: &Ctx<'_>, tab: &Tab) -> Value {
    let siblings: Vec<&Tab> = ctx
        .state
        .tabs
        .iter()
        .filter(|t| t.workspace_id == tab.workspace_id)
        .collect();
    let number = siblings
        .iter()
        .position(|t| t.tab_id == tab.tab_id)
        .map_or(1, |i| i + 1);
    let panes: Vec<&Pane> = ctx
        .state
        .panes
        .iter()
        .filter(|p| p.tab_id == tab.tab_id)
        .collect();
    json!({
        "tab_id": tab.tab_id,
        "workspace_id": tab.workspace_id,
        "number": number,
        "label": tab.label,
        "focused": false,
        "pane_count": panes.len(),
        "agent_status": rollup(ctx, &panes),
    })
}

pub(super) fn workspace_info(ctx: &Ctx<'_>, ws: &Workspace) -> Value {
    let number = ctx
        .state
        .workspaces
        .iter()
        .position(|w| w.workspace_id == ws.workspace_id)
        .map_or(1, |i| i + 1);
    let tabs: Vec<&Tab> = ctx
        .state
        .tabs
        .iter()
        .filter(|t| t.workspace_id == ws.workspace_id)
        .collect();
    let panes: Vec<&Pane> = ctx
        .state
        .panes
        .iter()
        .filter(|p| p.workspace_id == ws.workspace_id)
        .collect();
    let mut doc = Map::new();
    doc.insert("workspace_id".into(), json!(ws.workspace_id));
    doc.insert("number".into(), json!(number));
    doc.insert("label".into(), json!(ws.label));
    doc.insert("focused".into(), json!(false));
    doc.insert("pane_count".into(), json!(panes.len()));
    doc.insert("tab_count".into(), json!(tabs.len()));
    doc.insert(
        "active_tab_id".into(),
        json!(tabs.first().map(|t| t.tab_id.clone()).unwrap_or_default()),
    );
    doc.insert("agent_status".into(), json!(rollup(ctx, &panes)));
    Value::Object(doc)
}
