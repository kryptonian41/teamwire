//! Workspaces, tabs and panes: where agents live.

use serde_json::{Value, json};

use super::args::Args;
use super::state::{State, Tab, Workspace, pane_dir};
use super::{CliError, Ctx, fail};
use crate::fsutil;

fn workspace_json(w: &Workspace) -> Value {
    json!({ "workspace_id": w.workspace_id, "label": w.label })
}

fn tab_json(t: &Tab) -> Value {
    json!({ "tab_id": t.tab_id, "workspace_id": t.workspace_id, "label": t.label })
}

pub(super) fn pane_json(ctx: &Ctx<'_>, pane_id: &str) -> Value {
    let Some(p) = ctx.state.pane(pane_id) else {
        return Value::Null;
    };
    json!({ "pane_id": p.pane_id, "tab_id": p.tab_id, "workspace_id": p.workspace_id, "cwd": p.cwd })
}

fn add_pane(state: &mut State, ws: &str, tab: &str, cwd: &str) -> String {
    let id = format!("{ws}:p{}", state.next_id());
    state.panes.push(super::state::Pane {
        pane_id: id.clone(),
        tab_id: tab.to_owned(),
        workspace_id: ws.to_owned(),
        cwd: cwd.to_owned(),
        agent: None,
    });
    id
}

fn default_cwd(args: &Args) -> String {
    args.value("cwd").map_or_else(
        || {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        },
        str::to_owned,
    )
}

pub(super) fn workspace_create(ctx: &mut Ctx<'_>, args: &Args) -> Value {
    let cwd = default_cwd(args);
    let ws = format!("w{}", ctx.state.next_id());
    let tab = format!("{ws}:t{}", ctx.state.next_id());
    ctx.state.workspaces.push(Workspace {
        workspace_id: ws.clone(),
        label: args.value("label").unwrap_or("").to_owned(),
    });
    ctx.state.tabs.push(Tab {
        tab_id: tab.clone(),
        workspace_id: ws.clone(),
        label: String::new(),
    });
    let pane = add_pane(ctx.state, &ws, &tab, &cwd);
    json!({
        "type": "workspace_created",
        "workspace": ctx.state.workspaces.last().map(workspace_json),
        "tab": ctx.state.tabs.last().map(tab_json),
        "root_pane": pane_json(ctx, &pane),
    })
}

pub(super) fn workspace_get(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    match ctx.state.workspaces.iter().find(|w| w.workspace_id == id) {
        Some(w) => Ok(json!({ "type": "workspace_info", "workspace": workspace_json(w) })),
        None => Err(fail("workspace_not_found", format!("no workspace {id}"))),
    }
}

pub(super) fn workspace_rename(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    let label = args.operand(1).unwrap_or_default();
    match ctx
        .state
        .workspaces
        .iter_mut()
        .find(|w| w.workspace_id == id)
    {
        Some(w) => {
            label.clone_into(&mut w.label);
            Ok(json!({ "type": "workspace_renamed", "workspace": workspace_json(w) }))
        }
        None => Err(fail("workspace_not_found", format!("no workspace {id}"))),
    }
}

pub(super) fn tab_create(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let ws = args
        .value("workspace")
        .ok_or_else(|| fail("invalid_params", "--workspace is required"))?
        .to_owned();
    if !ctx.state.workspaces.iter().any(|w| w.workspace_id == ws) {
        return Err(fail("workspace_not_found", format!("no workspace {ws}")));
    }
    let tab = format!("{ws}:t{}", ctx.state.next_id());
    ctx.state.tabs.push(Tab {
        tab_id: tab.clone(),
        workspace_id: ws.clone(),
        label: args.value("label").unwrap_or("").to_owned(),
    });
    let pane = add_pane(ctx.state, &ws, &tab, &default_cwd(args));
    Ok(json!({
        "type": "tab_created",
        "tab": ctx.state.tabs.last().map(tab_json),
        "root_pane": pane_json(ctx, &pane),
    }))
}

pub(super) fn tab_rename(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    let label = args.operand(1).unwrap_or_default();
    match ctx.state.tabs.iter_mut().find(|t| t.tab_id == id) {
        Some(t) => {
            label.clone_into(&mut t.label);
            Ok(json!({ "type": "tab_renamed", "tab": tab_json(t) }))
        }
        None => Err(fail("tab_not_found", format!("no tab {id}"))),
    }
}

pub(super) fn pane_list(ctx: &Ctx<'_>, args: &Args) -> Value {
    let only = args.value("workspace");
    let ids: Vec<String> = ctx
        .state
        .panes
        .iter()
        .filter(|p| only.is_none_or(|w| p.workspace_id == w))
        .map(|p| p.pane_id.clone())
        .collect();
    let panes: Vec<Value> = ids.iter().map(|id| pane_json(ctx, id)).collect();
    json!({ "type": "pane_list", "panes": panes })
}

pub(super) fn pane_get(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    if ctx.state.pane(id).is_none() {
        return Err(fail("pane_not_found", format!("no pane {id}")));
    }
    Ok(json!({ "type": "pane_info", "pane": pane_json(ctx, id) }))
}

/// Closing the last pane of a tab closes the tab, and the last tab of a
/// workspace closes the workspace, as herdr does.
pub(super) fn pane_close(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default().to_owned();
    let Some(pane) = ctx.state.pane(&id).cloned() else {
        return Err(fail("pane_not_found", format!("no pane {id}")));
    };
    if let Some(agent) = &pane.agent {
        fsutil::terminate(agent.pid);
    }
    std::fs::remove_dir_all(pane_dir(ctx.state_path, &id)).ok();
    ctx.state.panes.retain(|p| p.pane_id != id);
    if !ctx.state.panes.iter().any(|p| p.tab_id == pane.tab_id) {
        ctx.state.tabs.retain(|t| t.tab_id != pane.tab_id);
    }
    if !ctx
        .state
        .tabs
        .iter()
        .any(|t| t.workspace_id == pane.workspace_id)
    {
        ctx.state
            .workspaces
            .retain(|w| w.workspace_id != pane.workspace_id);
    }
    Ok(json!({ "type": "pane_closed", "pane_id": id }))
}
