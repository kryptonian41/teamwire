//! The `agent` commands: start a fake Claude in a pane, type into it, read
//! its screen, and answer its dialogs.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};

use super::args::Args;
use super::state::{AgentRec, Pane, pane_dir};
use super::{CliError, Ctx, fail};
use crate::fsutil;

/// The states `status.json` holds, as `fake-claude` writes them.
const SETTLED: [&str; 3] = ["idle", "done", "blocked"];

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

fn env_millis(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The agent's status and its change counter.
fn read_status(dir: &Path) -> (String, u64) {
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

fn claude_dir() -> Option<PathBuf> {
    ["TEAMWIRE_CLAUDE_DIR", "FAKE_CLAUDE_DIR"]
        .iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
}

/// The session the agent reports: its session file if it wrote one, else the
/// id on its command line.
fn session_of(agent: &AgentRec) -> Option<String> {
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

fn alive(agent: &AgentRec) -> bool {
    fsutil::pid_alive(agent.pid)
}

fn agent_json(ctx: &Ctx<'_>, pane: &Pane, agent: &AgentRec) -> Value {
    let (status, seq) = read_status(&pane_dir(ctx.state_path, &pane.pane_id));
    let mut doc = json!({
        "agent": agent.kind,
        "agent_status": status,
        "cwd": pane.cwd,
        "focused": false,
        "foreground_cwd": pane.cwd,
        "interactive_ready": status == "idle",
        "name": agent.name,
        "pane_id": pane.pane_id,
        "revision": seq,
        "state_change_seq": seq,
        "tab_id": pane.tab_id,
        "terminal_id": format!("term_{}", pane.pane_id.replace(':', "")),
        "workspace_id": pane.workspace_id,
    });
    if let (Some(session), Some(map)) = (session_of(agent), doc.as_object_mut()) {
        map.insert(
            "agent_session".to_owned(),
            json!({
                "agent": agent.kind,
                "kind": "id",
                "source": "herdr:claude",
                "value": session,
            }),
        );
    }
    doc
}

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
    // A cleared environment, as herdr gives an agent in effect: the identity
    // of a member must arrive through its own config, not from here.
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
        .env("FAKE_PANE_ID", &pane.pane_id);
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
            "agent_kind_unsupported",
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
    let pane = ctx
        .state
        .pane(&pane_id)
        .cloned()
        .ok_or_else(|| fail("pane_not_found", format!("no pane {pane_id}")))?;
    if pane.agent.as_ref().is_some_and(alive) {
        return Err(fail("agent_pane_busy", "the pane already runs an agent"));
    }
    let name_taken = ctx.state.panes.iter().any(|p| {
        p.pane_id != pane_id && p.agent.as_ref().is_some_and(|a| a.name == name && alive(a))
    });
    if name_taken {
        return Err(fail(
            "agent_name_in_use",
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
    Ok(json!({ "type": "agent_started", "agent": agent_json(ctx, &pane, &rec) }))
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
    Ok(json!({ "type": "agent_info", "agent": agent_json(ctx, &pane, &agent) }))
}

pub(super) fn list(ctx: &Ctx<'_>) -> Value {
    let agents: Vec<Value> = ctx
        .state
        .panes
        .iter()
        .filter_map(|p| {
            let a = p.agent.as_ref().filter(|a| alive(a))?;
            Some(agent_json(ctx, p, a))
        })
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
    Ok(json!({
        "type": "agent_read",
        "source": args.value("source").unwrap_or("recent"),
        "text": text,
    }))
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
    Ok(json!({ "type": "agent_keys_sent", "keys": keys }))
}

/// Types the prompt into the agent. With `stalled`, does everything but
/// report success: the text is delivered and the call fails with
/// `agent_prompt_stalled`.
pub(super) fn prompt(ctx: &mut Ctx<'_>, args: &Args, stalled: bool) -> Result<Value, CliError> {
    let target = args.operand(0).unwrap_or_default();
    let text = args.operand(1).unwrap_or_default();
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
        "type": "agent_prompt_submitted",
        "agent": agent_json(ctx, &pane, &agent),
    }))
}

/// Waits for activity after a prompt, then for the agent to settle.
fn wait_for_turn(dir: &Path, before_seq: u64, timeout_ms: Option<u64>) -> Result<(), CliError> {
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
