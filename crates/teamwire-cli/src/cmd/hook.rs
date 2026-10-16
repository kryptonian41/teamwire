//! `teamwire hook`: the per-member Stop and `PostToolUse` hooks and the lead's
//! Stop hook; always exits 0.

use std::io::{Read, Write};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{Config, Role, TeamName, TurnEnd, TurnEndReply};
use teamwire_store::{Ctx, Store};

use crate::cmd::send::state_home;
use crate::wiring::{Wiring, claude_harness_id};
use crate::{Global, HookArgs, HookCommand, HookTarget};

/// The lead's Stop hook runs on every stop of every Claude session where the
/// plugin is on, so a slow disk or a locked store must not hold the session.
const LEAD_STOP_BUDGET: Duration = Duration::from_secs(5);

/// More hook input than this is not read; Claude Code's is a few kilobytes.
const STDIN_LIMIT: u64 = 1 << 20;

pub(crate) fn run(args: &HookArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    // A hook never fails the harness turn: every path below returns here.
    let reply = match &args.hook {
        HookCommand::ClaudeStop(t) => member_hook(t, false, global, wiring),
        HookCommand::ClaudePostTool(t) => member_hook(t, true, global, wiring),
        HookCommand::ClaudeLeadStop => lead_stop_hook(global, wiring),
    };
    if let Some(reply) = reply {
        write_stdout(&reply);
    }
    ExitCode::SUCCESS
}

/// The bytes to print, or `None` for a hook with nothing to say.
fn member_hook(
    target: &HookTarget,
    post_tool: bool,
    global: &Global,
    wiring: &Wiring,
) -> Option<Vec<u8>> {
    // A session that is not a team member leaves before opening any file.
    let team_var = std::env::var("TEAMWIRE_TEAM")
        .ok()
        .filter(|v| !v.is_empty())?;
    let role_var = std::env::var("TEAMWIRE_ROLE")
        .ok()
        .filter(|v| !v.is_empty())?;
    // The command line names the same member as the environment, or this is
    // not that member's session (ADR 0008 point 4).
    if target.team.as_deref().is_some_and(|t| t != team_var)
        || target.role.as_deref().is_some_and(|r| r != role_var)
    {
        return None;
    }
    let team = TeamName::parse(&team_var).ok()?;
    let role = Role::parse(&role_var).ok()?;

    let mut input = Vec::new();
    std::io::stdin()
        .lock()
        .take(STDIN_LIMIT)
        .read_to_end(&mut input)
        .ok()?;
    let harness = wiring.registry.harness(&claude_harness_id()).ok()?;
    let turn = harness.parse_turn_end(&input).ok()?;

    let home = state_home(global)?;
    let mut store = Store::open_existing(&home, &team).ok()?;
    let config = Config::default();
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    if post_tool {
        post_tool_reply(&mut ctx, &role, &turn)
    } else {
        match teamwire_report::on_turn_end(&mut ctx, &role, &turn) {
            TurnEndReply::Continue => None,
            TurnEndReply::Hold(note) => Some(harness.hold_reply(&note)),
        }
    }
}

/// The lead's Stop hook (ADR 0018 point 5): hold the stop once, naming each
/// team that has working members and no running watch. It reads stores and
/// writes nothing.
fn lead_stop_hook(global: &Global, wiring: &Wiring) -> Option<Vec<u8>> {
    // A member never runs this hook; the plugin is also switched off for it.
    if std::env::var_os("TEAMWIRE_TEAM").is_some() {
        return None;
    }
    let mut input = Vec::new();
    std::io::stdin()
        .lock()
        .take(STDIN_LIMIT)
        .read_to_end(&mut input)
        .ok()?;
    let harness = wiring.registry.harness(&claude_harness_id()).ok()?;
    let turn = harness.parse_turn_end(&input).ok()?;
    if turn.subagent || turn.hold_already_active {
        return None;
    }
    let home = state_home(global)?;
    let platform = wiring.platform.clone();
    let clock = wiring.clock;
    let (tx, rx) = mpsc::channel();
    // The thread is left behind when the budget runs out; the process exits
    // right after, which ends it.
    std::thread::spawn(move || {
        let teams = teamwire_report::teams_without_watch(&home, &turn.session, &*platform, &clock);
        tx.send(teams).ok();
    });
    let teams = rx.recv_timeout(LEAD_STOP_BUDGET).ok()?;
    if teams.is_empty() {
        return None;
    }
    let note: Vec<String> = teams
        .iter()
        .map(teamwire_report::watch_missing_note)
        .collect();
    Some(harness.hold_reply(&note.join("\n")))
}

/// The in-turn reminder, for the member's own session only.
fn post_tool_reply(ctx: &mut Ctx<'_>, role: &Role, turn: &TurnEnd) -> Option<Vec<u8>> {
    if turn.subagent {
        return None;
    }
    let member = ctx.store.read(|tx| tx.member(role)).ok().flatten()?;
    if member.session_id.as_ref() != Some(&turn.session) {
        return None;
    }
    let note = teamwire_report::on_tool_use(ctx, role)?;
    serde_json::to_vec(&json!({
        "hookSpecificOutput": { "hookEventName": "PostToolUse", "additionalContext": note }
    }))
    .ok()
}

fn write_stdout(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    // A closed pipe cannot fail the harness turn.
    if out.write_all(bytes).and_then(|()| out.flush()).is_err() {
        tracing::debug!("the hook reply could not be written");
    }
}
