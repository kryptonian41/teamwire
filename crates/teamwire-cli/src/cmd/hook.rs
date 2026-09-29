//! `teamwire hook`: the per-member Stop and `PostToolUse` hooks; always exits 0.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::json;
use teamwire_core::{Config, HarnessId, Role, TeamName, TurnEnd, TurnEndReply};
use teamwire_harness_claude::ClaudeCode;
use teamwire_store::{Ctx, Store};

use crate::wiring::Wiring;
use crate::{Global, HookArgs, HookCommand, HookTarget};

/// More hook input than this is not read; Claude Code's is a few kilobytes.
const STDIN_LIMIT: u64 = 1 << 20;

pub(crate) fn run(args: &HookArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    // A hook never fails the harness turn: every path below returns here.
    let (target, post_tool) = match &args.hook {
        HookCommand::ClaudeStop(t) => (t, false),
        HookCommand::ClaudePostTool(t) => (t, true),
    };
    if let Some(reply) = member_hook(target, post_tool, global, wiring) {
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
    let harness = wiring
        .registry
        .harness(&HarnessId::from_static(ClaudeCode::ID))
        .ok()?;
    let turn = harness.parse_turn_end(&input).ok()?;

    let home = state_home(global);
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

/// `--home`, else `$TEAMWIRE_HOME`, else `~/.teamwire`.
fn state_home(global: &Global) -> PathBuf {
    let Global { home } = global;
    home.clone()
        .or_else(|| std::env::var_os("TEAMWIRE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".teamwire")))
        .unwrap_or_else(|| PathBuf::from(".teamwire"))
}

fn write_stdout(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    // A closed pipe cannot fail the harness turn.
    if out.write_all(bytes).and_then(|()| out.flush()).is_err() {
        tracing::debug!("the hook reply could not be written");
    }
}
