//! The `agent` commands: start a fake Claude in a pane, type into it, read
//! its screen, and answer its dialogs.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};

use super::args::Args;
use super::pane_files::{alive, claude_dir, env_millis, read_status, wait_for_turn};
use super::shapes::{agent_info, live_agent as running};
use super::state::{AgentRec, Pane, pane_dir};
use super::{CliError, Ctx, fail};
use crate::fsutil;

/// Keys herdr accepts besides a single character.
const KEY_NAMES: [&str; 8] = [
    "enter",
    "esc",
    "tab",
    "backspace",
    "up",
    "down",
    "left",
    "right",
];

/// The pane and agent for a name or pane id, if the agent is still running.
fn live_agent(ctx: &Ctx<'_>, target: &str) -> Result<(Pane, AgentRec), CliError> {
    let missing = || fail("agent_not_found", format!("no agent {target}"));
    let pane = ctx.state.find_agent_pane(target).ok_or_else(missing)?;
    let agent = pane.agent.clone().ok_or_else(missing)?;
    if alive(&agent) {
        Ok((pane.clone(), agent))
    } else {
        Err(missing())
    }
}

/// The program `--kind claude` runs: `FAKE_CLAUDE_BIN`, else the
/// `fake-claude` next to this binary, else `fake-claude` on `PATH`.
fn claude_program() -> PathBuf {
    if let Some(p) = std::env::var_os("FAKE_CLAUDE_BIN") {
        return PathBuf::from(p);
    }
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("fake-claude")));
    match sibling {
        Some(p) if p.is_file() => p,
        _ => PathBuf::from("fake-claude"),
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "fake-herdr is the fake terminal host: starting the agent process is its whole job"
)]
fn spawn_agent(dir: &Path, pane: &Pane, tail: &[String]) -> Result<u32, CliError> {
    std::fs::create_dir_all(dir).map_err(|e| fail("internal_error", e.to_string()))?;
    let screen = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(dir.join("screen.txt"))
        .map_err(|e| fail("internal_error", e.to_string()))?;
    let screen_err = screen
        .try_clone()
        .map_err(|e| fail("internal_error", e.to_string()))?;
    let mut cmd = std::process::Command::new(claude_program());
    cmd.args(tail).env_clear();
    // A cleared environment plus the pane's HERDR_* variables. A real pane
    // also has the login environment, so this is stricter on purpose: a member's identity
    // must arrive through its own config, and a test fails if it does not.
    for name in ["PATH", "HOME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    for (name, value) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("FAKE_") {
            cmd.env(name, value);
        }
    }
    if let Some(dir) = claude_dir() {
        cmd.env("FAKE_CLAUDE_DIR", dir);
    }
    cmd.env("FAKE_PANE_DIR", dir)
        .env("FAKE_PANE_ID", &pane.pane_id)
        // Real herdr gives every pane these, and the locator reads
        // HERDR_PANE_ID from the session's process to find its place.
        .env("HERDR_ENV", "1")
        .env("HERDR_PANE_ID", &pane.pane_id)
        .env("HERDR_TAB_ID", &pane.tab_id)
        .env("HERDR_WORKSPACE_ID", &pane.workspace_id);
    if Path::new(&pane.cwd).is_dir() {
        cmd.current_dir(&pane.cwd);
    }
    let child = std::os::unix::process::CommandExt::process_group(&mut cmd, 0)
        .stdin(Stdio::null())
        .stdout(Stdio::from(screen))
        .stderr(Stdio::from(screen_err))
        .spawn()
        .map_err(|e| fail("agent_start_failed", e.to_string()))?;
    Ok(child.id())
}

pub(super) fn start(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let name = args
        .operand(0)
        .ok_or_else(|| fail("invalid_params", "agent start needs a name"))?
        .to_owned();
    let kind = args.value("kind").unwrap_or("claude");
    if kind != "claude" {
        return Err(fail(
            "unsupported_agent_kind",
            format!("fake-herdr starts only claude, not {kind}"),
        ));
    }
    let pane_id = args
        .value("pane")
        .ok_or_else(|| fail("invalid_params", "--pane is required"))?
        .to_owned();
    if args.tail.iter().any(|a| a.contains('\n')) {
        return Err(fail(
            "invalid_agent_argument",
            "argument cannot be encoded safely for the target shell",
        ));
    }
    let pane = ctx.state.pane(&pane_id).cloned().ok_or_else(|| {
        fail(
            "agent_pane_not_found",
            format!("agent target pane {pane_id} not found"),
        )
    })?;
    if let Some(ms) = args.millis("timeout")
        && !(3_001..=300_000).contains(&ms)
    {
        return Err(fail(
            "invalid_timeout",
            "timeout must be greater than 3000 and at most 300000 milliseconds",
        ));
    }
    if pane.agent.as_ref().is_some_and(alive) {
        return Err(fail("agent_pane_busy", "the pane already runs an agent"));
    }
    let name_taken = ctx.state.panes.iter().any(|p| {
        p.pane_id != pane_id && p.agent.as_ref().is_some_and(|a| a.name == name && alive(a))
    });
    if name_taken {
        return Err(fail(
            "agent_name_taken",
            format!("an agent named {name} is already live"),
        ));
    }
    let dir = pane_dir(ctx.state_path, &pane_id);
    std::fs::remove_dir_all(&dir).ok();
    let pid = spawn_agent(&dir, &pane, &args.tail)?;
    let hint = args
        .tail
        .windows(2)
        .find(|w| {
            matches!(
                w.first().map(String::as_str),
                Some("--session-id" | "--resume")
            )
        })
        .and_then(|w| w.get(1).cloned());
    let rec = AgentRec {
        name,
        kind: kind.to_owned(),
        pid,
        session_hint: hint,
        fixed: false,
    };
    if let Some(p) = ctx.state.panes.iter_mut().find(|p| p.pane_id == pane_id) {
        p.agent = Some(rec.clone());
    }
    let limit = args
        .millis("timeout")
        .unwrap_or(30_000)
        .min(env_millis("FAKE_HERDR_START_WAIT_MS", 10_000));
    let started = fsutil::Stopwatch::start();
    loop {
        let (status, _) = read_status(&dir);
        match status.as_str() {
            "idle" => break,
            "blocked" => {
                return Err(fail(
                    "agent_not_ready",
                    "the agent is blocked at a startup dialog; read it and send keys",
                ));
            }
            "exited" => return Err(fail("agent_not_ready", "the agent exited during startup")),
            _ => {}
        }
        if started.elapsed() > Duration::from_millis(limit) {
            return Err(fail("timeout", "the agent was not ready in time"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let pane = pane_now(ctx, &pane_id)?;
    let mut argv = vec![claude_program().display().to_string()];
    argv.extend(args.tail.iter().cloned());
    Ok(json!({
        "type": "agent_started",
        "agent": agent_info(ctx, &pane, &rec),
        "argv": argv,
    }))
}

fn pane_now(ctx: &Ctx<'_>, pane_id: &str) -> Result<Pane, CliError> {
    ctx.state
        .pane(pane_id)
        .cloned()
        .ok_or_else(|| fail("pane_not_found", format!("no pane {pane_id}")))
}

pub(super) fn get(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let target = args.operand(0).unwrap_or_default();
    let (pane, agent) = live_agent(ctx, target)?;
    Ok(json!({ "type": "agent_info", "agent": agent_info(ctx, &pane, &agent) }))
}

pub(super) fn list(ctx: &Ctx<'_>) -> Value {
    let agents: Vec<Value> = ctx
        .state
        .panes
        .iter()
        .filter_map(|p| Some(agent_info(ctx, p, running(p)?)))
        .collect();
    json!({ "type": "agent_list", "agents": agents })
}

pub(super) fn read(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let target = args.operand(0).unwrap_or_default();
    let pane = ctx
        .state
        .find_agent_pane(target)
        .filter(|p| p.agent.is_some())
        .cloned()
        .ok_or_else(|| fail("agent_not_found", format!("no agent {target}")))?;
    let lines: usize = args
        .value("lines")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let screen = fsutil::read_text(&pane_dir(ctx.state_path, &pane.pane_id).join("screen.txt"))
        .unwrap_or_default();
    let total = screen.lines().count();
    let text = screen
        .lines()
        .skip(total.saturating_sub(lines))
        .collect::<Vec<_>>()
        .join("\n");
    // Real herdr prints the screen itself, not a JSON result; `run` prints
    // this string as it is.
    Ok(Value::String(text))
}

pub(super) fn send_keys(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let target = args.operand(0).unwrap_or_default();
    let keys: Vec<&str> = args.words.iter().skip(3).map(String::as_str).collect();
    if keys.is_empty() {
        return Err(fail(
            "invalid_params",
            "agent send-keys needs at least one key",
        ));
    }
    if let Some(bad) = keys
        .iter()
        .find(|k| !(KEY_NAMES.contains(k) || **k == "ctrl+c" || k.chars().count() == 1))
    {
        return Err(fail("invalid_key", format!("unknown key {bad}")));
    }
    let (pane, _) = live_agent(ctx, target)?;
    let dir = pane_dir(ctx.state_path, &pane.pane_id);
    let mut queued = String::new();
    for k in &keys {
        queued.push_str(k);
        queued.push('\n');
    }
    fsutil::append(&dir.join("keys"), queued.as_bytes())
        .map_err(|e| fail("internal_error", e.to_string()))?;
    Ok(json!({ "type": "ok" }))
}

/// Types the prompt into the agent. With `stalled`, does everything but
/// report success: the text is delivered and the call fails with
/// `agent_prompt_stalled`.
pub(super) fn prompt(ctx: &mut Ctx<'_>, args: &Args, stalled: bool) -> Result<Value, CliError> {
    let target = args.operand(0).unwrap_or_default();
    let text = args.operand(1).unwrap_or_default();
    if text.trim().is_empty() {
        return Err(fail("empty_agent_prompt", "agent prompt must not be empty"));
    }
    let (pane, agent) = live_agent(ctx, target)?;
    let dir = pane_dir(ctx.state_path, &pane.pane_id);
    let (before_status, before_seq) = read_status(&dir);
    if before_status == "blocked" {
        return Err(fail(
            "agent_blocked",
            "the agent is waiting at a dialog; nothing was sent",
        ));
    }
    let line = format!("{}\n", Value::String(text.to_owned()));
    fsutil::append(&dir.join("inbox"), line.as_bytes())
        .map_err(|e| fail("internal_error", e.to_string()))?;
    if stalled {
        return Err(fail(
            "agent_prompt_stalled",
            "no agent activity was seen after the prompt was submitted",
        ));
    }
    if args.has("wait") {
        wait_for_turn(&dir, before_seq, args.millis("timeout"))?;
    }
    Ok(json!({
        "type": "agent_prompted",
        "agent": agent_info(ctx, &pane, &agent),
    }))
}

/// Fake-only: makes a workspace with one pane that runs an agent with no
/// name and no process, `idle`, as real herdr lists agents it detected
/// itself. Returns the agent's info.
pub(super) fn add_unnamed_agent(ctx: &mut Ctx<'_>) -> Value {
    let ws = format!("w{}", ctx.state.next_id());
    let tab = format!("{ws}:t{}", ctx.state.next_id());
    let pane_id = format!("{ws}:p{}", ctx.state.next_id());
    ctx.state.workspaces.push(super::state::Workspace {
        workspace_id: ws.clone(),
        label: String::new(),
    });
    ctx.state.tabs.push(super::state::Tab {
        tab_id: tab.clone(),
        workspace_id: ws.clone(),
        label: String::new(),
    });
    let agent = AgentRec {
        name: String::new(),
        kind: "claude".to_owned(),
        pid: 0,
        session_hint: Some("unnamed-session".to_owned()),
        fixed: true,
    };
    let pane = Pane {
        pane_id: pane_id.clone(),
        tab_id: tab,
        workspace_id: ws,
        cwd: "/tmp".to_owned(),
        label: None,
        agent: Some(agent.clone()),
    };
    ctx.state.panes.push(pane.clone());
    let dir = pane_dir(ctx.state_path, &pane_id);
    std::fs::create_dir_all(&dir).ok();
    fsutil::write_atomic(&dir.join("status.json"), br#"{"status":"idle","seq":1}"#).ok();
    json!({ "type": "agent_info", "agent": agent_info(ctx, &pane, &agent) })
}
