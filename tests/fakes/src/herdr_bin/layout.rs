//! Workspaces, tabs and panes: where agents live.

use serde_json::{Value, json};

use super::args::Args;
use super::shapes::{pane_info, tab_info, workspace_info};
use super::state::{Pane, State, Tab, Workspace, pane_dir};
use super::{CliError, Ctx, fail};

fn add_pane(state: &mut State, ws: &str, tab: &str, cwd: &str) -> String {
    let id = format!("{ws}:p{}", state.next_id());
    state.panes.push(Pane {
        pane_id: id.clone(),
        tab_id: tab.to_owned(),
        workspace_id: ws.to_owned(),
        cwd: cwd.to_owned(),
        label: None,
        agent: None,
        env: Vec::new(),
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

fn root_pane(ctx: &Ctx<'_>, id: &str) -> Value {
    ctx.state
        .pane(id)
        .map_or(Value::Null, |p| pane_info(ctx, p))
}

pub(super) fn workspace_create(ctx: &mut Ctx<'_>, args: &Args) -> Value {
    let cwd = default_cwd(args);
    let ws = format!("w{}", ctx.state.next_id());
    let tab = format!("{ws}:t{}", ctx.state.next_id());
    let workspace = Workspace {
        workspace_id: ws.clone(),
        label: args.value("label").unwrap_or("").to_owned(),
    };
    let first_tab = Tab {
        tab_id: tab.clone(),
        workspace_id: ws.clone(),
        label: String::new(),
    };
    ctx.state.workspaces.push(workspace.clone());
    ctx.state.tabs.push(first_tab.clone());
    let pane = add_pane(ctx.state, &ws, &tab, &cwd);
    json!({
        "type": "workspace_created",
        "workspace": workspace_info(ctx, &workspace),
        "tab": tab_info(ctx, &first_tab),
        "root_pane": root_pane(ctx, &pane),
    })
}

pub(super) fn workspace_list(ctx: &Ctx<'_>) -> Value {
    let all: Vec<Value> = ctx
        .state
        .workspaces
        .iter()
        .map(|w| workspace_info(ctx, w))
        .collect();
    json!({ "type": "workspace_list", "workspaces": all })
}

fn find_workspace<'a>(ctx: &'a Ctx<'_>, id: &str) -> Result<&'a Workspace, CliError> {
    ctx.state
        .workspaces
        .iter()
        .find(|w| w.workspace_id == id)
        .ok_or_else(|| fail("workspace_not_found", format!("no workspace {id}")))
}

pub(super) fn workspace_get(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let ws = find_workspace(ctx, args.operand(0).unwrap_or_default())?;
    Ok(json!({ "type": "workspace_info", "workspace": workspace_info(ctx, ws) }))
}

pub(super) fn workspace_rename(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    let label = args.operand(1).unwrap_or_default();
    find_workspace(ctx, id)?;
    if let Some(w) = ctx
        .state
        .workspaces
        .iter_mut()
        .find(|w| w.workspace_id == id)
    {
        label.clone_into(&mut w.label);
    }
    workspace_get(ctx, args)
}

pub(super) fn tab_create(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let ws = args
        .value("workspace")
        .ok_or_else(|| fail("invalid_params", "--workspace is required"))?
        .to_owned();
    find_workspace(ctx, &ws)?;
    let tab = format!("{ws}:t{}", ctx.state.next_id());
    let created = Tab {
        tab_id: tab.clone(),
        workspace_id: ws.clone(),
        label: args.value("label").unwrap_or("").to_owned(),
    };
    ctx.state.tabs.push(created.clone());
    let pane = add_pane(ctx.state, &ws, &tab, &default_cwd(args));
    Ok(json!({
        "type": "tab_created",
        "tab": tab_info(ctx, &created),
        "root_pane": root_pane(ctx, &pane),
    }))
}

pub(super) fn tab_rename(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    let label = args.operand(1).unwrap_or_default();
    let Some(tab) = ctx.state.tabs.iter_mut().find(|t| t.tab_id == id) else {
        return Err(fail("tab_not_found", format!("no tab {id}")));
    };
    label.clone_into(&mut tab.label);
    let renamed = tab.clone();
    Ok(json!({ "type": "tab_info", "tab": tab_info(ctx, &renamed) }))
}

pub(super) fn pane_list(ctx: &Ctx<'_>, args: &Args) -> Value {
    let only = args.value("workspace");
    let panes: Vec<Value> = ctx
        .state
        .panes
        .iter()
        .filter(|p| only.is_none_or(|w| p.workspace_id == w))
        .map(|p| pane_info(ctx, p))
        .collect();
    json!({ "type": "pane_list", "panes": panes })
}

fn find_pane<'a>(ctx: &'a Ctx<'_>, id: &str) -> Result<&'a Pane, CliError> {
    ctx.state
        .pane(id)
        .ok_or_else(|| fail("pane_not_found", format!("no pane {id}")))
}

pub(super) fn pane_get(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let pane = find_pane(ctx, args.operand(0).unwrap_or_default())?;
    Ok(json!({ "type": "pane_info", "pane": pane_info(ctx, pane) }))
}

/// A new pane in the tab of the pane it splits, as herdr makes one, with the
/// split's working directory (else the split pane's) and its `--env` pairs.
/// The fake ignores `--direction` and `--ratio`: it draws no layout.
pub(super) fn pane_split(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let target = args
        .operand(0)
        .or_else(|| args.value("pane"))
        .ok_or_else(|| fail("invalid_params", "fake-herdr splits only a named pane"))?
        .to_owned();
    let old = find_pane(ctx, &target)?.clone();
    let mut env = Vec::new();
    for pair in args.all("env") {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| fail("invalid_params", format!("--env {pair} is not KEY=VALUE")))?;
        env.push((key.to_owned(), value.to_owned()));
    }
    let cwd = args
        .value("cwd")
        .map_or_else(|| old.cwd.clone(), str::to_owned);
    let id = add_pane(ctx.state, &old.workspace_id, &old.tab_id, &cwd);
    if let Some(p) = ctx.state.panes.iter_mut().find(|p| p.pane_id == id) {
        p.env = env;
    }
    let pane = find_pane(ctx, &id)?;
    Ok(json!({ "type": "pane_info", "pane": pane_info(ctx, pane) }))
}

pub(super) fn pane_rename(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default();
    find_pane(ctx, id)?;
    if let Some(p) = ctx.state.panes.iter_mut().find(|p| p.pane_id == id) {
        p.label = args.operand(1).map(str::to_owned);
    }
    pane_get(ctx, args)
}

/// Closing the last pane of a tab closes the tab, and the last tab of a
/// workspace closes the workspace, as herdr does.
pub(super) fn pane_close(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default().to_owned();
    let pane = find_pane(ctx, &id)?.clone();
    if let Some(agent) = pane.agent.as_ref().filter(|a| !a.fixed) {
        super::end_agent(ctx.state_path, &id, agent.pid);
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
    Ok(json!({ "type": "ok" }))
}
