//! `Host::spawn`: one workspace per team, one tab per member, then
//! `agent start` in the tab's first pane.

use std::time::Duration;

use teamwire_core::{HostError, HostErrorCode, Place, SpawnRequest};

use crate::Herdr;
use crate::shapes::{NewTab, new_tab};
use teamwire_core::Host;

/// The slack `agent start` gets on top of herdr's own `--timeout`, so herdr
/// answers before the process is killed.
const START_SLACK: Duration = Duration::from_secs(10);
/// herdr accepts an `agent start` timeout above this and up to `MAX_START_MS`.
const MIN_START_MS: u64 = 3_001;
const MAX_START_MS: u64 = 300_000;
/// Every other call in a spawn.
const LAYOUT_TIMEOUT: Duration = Duration::from_secs(30);

/// The kinds herdr detects that this adapter passes through; any other
/// program name starts `claude`.
const KINDS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];

/// The team part of `tw_<team>_<role>`. A team name has no underscore.
fn team_of(name: &str) -> &str {
    name.strip_prefix("tw_")
        .and_then(|rest| rest.split('_').next())
        .unwrap_or(name)
}

/// `argv[0]` is the program; herdr picks the program by `--kind`, so only
/// the arguments after it are passed on.
fn kind_and_args(argv: &[String]) -> (&'static str, &[String]) {
    match argv.split_first() {
        Some((first, rest)) if !first.starts_with('-') => {
            let base = first.rsplit('/').next().unwrap_or(first);
            let kind = KINDS.iter().find(|k| **k == base).copied();
            (kind.unwrap_or("claude"), rest)
        }
        _ => ("claude", argv),
    }
}

fn place_for(host: &Herdr, tab: &NewTab) -> Place {
    Place {
        host: host.id(),
        id: tab.pane.clone(),
    }
}

/// The workspace a live member of the team is in. A label match would pick a
/// user's workspace or a leftover one, so the live agents named
/// `tw_<team>_*` decide.
fn team_workspace(host: &Herdr, team: &str) -> Result<Option<String>, HostError> {
    let prefix = format!("tw_{team}_");
    Ok(host
        .agents(LAYOUT_TIMEOUT)?
        .iter()
        .find(|a| a.name.as_deref().is_some_and(|n| n.starts_with(&prefix)))
        .and_then(|a| a.workspace().map(str::to_owned)))
}

/// The tab for a new member: a new workspace labelled with the team when no
/// member of the team is live, a new tab labelled with the member's title
/// after that. Labels are set here and never changed.
fn make_tab(host: &Herdr, req: &SpawnRequest) -> Result<NewTab, HostError> {
    let team = team_of(req.name.as_str());
    let cwd = req.cwd.display().to_string();
    if let Some(ws) = team_workspace(host, team)? {
        let result = host.call(
            &[
                "tab",
                "create",
                "--workspace",
                &ws,
                "--cwd",
                &cwd,
                "--label",
                &req.title,
                "--no-focus",
            ],
            LAYOUT_TIMEOUT,
        )?;
        return new_tab(result);
    }
    let result = host.call(
        &[
            "workspace",
            "create",
            "--cwd",
            &cwd,
            "--label",
            team,
            "--no-focus",
        ],
        LAYOUT_TIMEOUT,
    )?;
    let tab = new_tab(result)?;
    host.call(&["tab", "rename", &tab.tab, &req.title], LAYOUT_TIMEOUT)?;
    Ok(tab)
}

/// Runs `agent start`, trying again while the pane is busy (not yet at its
/// shell prompt). `agent_not_ready` means the agent started and waits at a
/// dialog: it stays in the pane, so that is a started member, and the caller
/// answers the dialog. Retrying would only meet `agent_pane_busy`.
fn start(host: &Herdr, args: &[String], run_for: Duration) -> Result<(), HostError> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut last: Option<HostError> = None;
    for attempt in 0..host.start_tries {
        if attempt > 0 {
            std::thread::sleep(host.start_retry_delay);
        }
        match host.call(&refs, run_for) {
            Ok(_) => return Ok(()),
            Err(e) if e.code == HostErrorCode::NotReady => return Ok(()),
            Err(e) if e.code == HostErrorCode::Busy => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| HostError::new(HostErrorCode::Other, "agent start made no attempt")))
}

pub(crate) fn spawn(
    host: &Herdr,
    req: &SpawnRequest,
    timeout: Duration,
) -> Result<Place, HostError> {
    let tab = make_tab(host, req)?;
    let (kind, rest) = kind_and_args(&req.argv);
    let wait = u64::try_from(timeout.as_millis())
        .unwrap_or(MAX_START_MS)
        .clamp(MIN_START_MS, MAX_START_MS);
    let wait_ms = wait.to_string();
    let mut args: Vec<String> = [
        "agent",
        "start",
        req.name.as_str(),
        "--kind",
        kind,
        "--pane",
        &tab.pane,
        "--timeout",
        &wait_ms,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if !rest.is_empty() {
        args.push("--".to_owned());
        args.extend(rest.iter().cloned());
    }
    if let Err(e) = start(host, &args, Duration::from_millis(wait) + START_SLACK) {
        // Launch records a place only on success, so nothing else can close the
        // pane made above. Herdr closes the tab with its last pane, and the
        // workspace with its last tab. On a timeout this also stops an agent
        // that started late.
        drop(host.call(&["pane", "close", &tab.pane], LAYOUT_TIMEOUT));
        return Err(e);
    }
    Ok(place_for(host, &tab))
}
