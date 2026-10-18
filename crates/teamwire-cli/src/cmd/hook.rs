//! `teamwire hook`: the per-member Stop and `PostToolUse` hooks and the lead's
//! Stop hook; always exits 0.

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{Config, HarnessId, MemberState, Role, TeamName, TurnEnd, TurnEndReply};
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
        HookCommand::ClaudeStop(t) | HookCommand::TurnEnd(t) => {
            member_hook(t, MemberHook::TurnEnd, global, wiring)
        }
        HookCommand::ClaudePostTool(t) | HookCommand::PostTool(t) => {
            member_hook(t, MemberHook::PostTool, global, wiring)
        }
        HookCommand::SessionStart(t) => member_hook(t, MemberHook::SessionStart, global, wiring),
        HookCommand::ClaudeLeadStop => lead_stop_hook(None, global, wiring),
        HookCommand::LeadStop(a) => lead_stop_hook(a.harness.as_deref(), global, wiring),
    };
    if let Some(reply) = reply {
        write_stdout(&reply);
    }
    ExitCode::SUCCESS
}

/// Which member hook is running.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MemberHook {
    TurnEnd,
    PostTool,
    SessionStart,
}

/// The bytes to print, or `None` for a hook with nothing to say.
fn member_hook(
    target: &HookTarget,
    kind: MemberHook,
    global: &Global,
    wiring: &Wiring,
) -> Option<Vec<u8>> {
    // A session that is not a team member leaves before opening any file.
    let team_var = std::env::var("TEAMWIRE_TEAM")
        .ok()
        .filter(|v| !v.is_empty());
    let role_var = std::env::var("TEAMWIRE_ROLE")
        .ok()
        .filter(|v| !v.is_empty());
    // The command line names the same member as the environment, or this is
    // not that member's session (ADR 0008 point 4). A session-start hook may
    // have no member environment: its token proves who it is instead.
    let (team_var, role_var) = if kind == MemberHook::SessionStart {
        (
            target.team.clone().or(team_var)?,
            target.role.clone().or(role_var)?,
        )
    } else {
        let (team_var, role_var) = (team_var?, role_var?);
        if target.team.as_deref().is_some_and(|t| t != team_var)
            || target.role.as_deref().is_some_and(|r| r != role_var)
        {
            return None;
        }
        (team_var, role_var)
    };
    let team = TeamName::parse(&team_var).ok()?;
    let role = Role::parse(&role_var).ok()?;
    let home = state_home(global)?;
    let token = teamwire_msg::caller_token(|k| std::env::var(k).ok());
    let mut stdin = std::io::stdin().lock().take(STDIN_LIMIT);
    member_hook_in(
        &home,
        &team,
        &role,
        kind,
        token.as_deref(),
        wiring,
        &mut stdin,
    )
}

/// The store opens before standard input is parsed, because the member's own
/// harness parses it and the member row names that harness.
fn member_hook_in(
    home: &Path,
    team: &TeamName,
    role: &Role,
    kind: MemberHook,
    token: Option<&str>,
    wiring: &Wiring,
    stdin: &mut dyn Read,
) -> Option<Vec<u8>> {
    let mut store = Store::open_existing(home, team).ok()?;
    let found = store
        .read(|tx| Ok((tx.member(role)?, tx.member_token_hash(role)?)))
        .ok()?;
    let (member, stored_hash) = (found.0?, found.1);
    if member.state == MemberState::Removed {
        return None;
    }
    let harness = wiring.registry.harness(&member.harness).ok()?;
    if kind == MemberHook::PostTool && !harness.capabilities().in_turn_hook {
        return None;
    }
    let mut input = Vec::new();
    stdin.read_to_end(&mut input).ok()?;
    let turn = harness.parse_turn_end(&input).ok()?;

    if kind == MemberHook::SessionStart {
        // Only the member's own token may name its session (ADR 0023 point 5).
        let stored = stored_hash?;
        if !same_bytes(&teamwire_msg::token_hash(token?), &stored) {
            return None;
        }
        if let Err(e) = store.write(|tx| tx.set_member_session(role, &turn.session)) {
            tracing::warn!(error = %e, %role, "could not record the session");
        }
        return None;
    }

    let config = Config::default();
    let mut ctx = Ctx {
        home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    if kind == MemberHook::PostTool {
        post_tool_reply(&mut ctx, role, &turn)
    } else {
        match teamwire_report::on_turn_end(&mut ctx, role, &turn) {
            TurnEndReply::Continue => None,
            TurnEndReply::Hold(note) => Some(harness.hold_reply(&note)),
        }
    }
}

/// Compares two hashes without stopping at the first difference.
fn same_bytes(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The lead's Stop hook (ADR 0018 point 5): hold the stop once, naming each
/// team that has working members and no running watch. It reads stores and
/// writes nothing.
fn lead_stop_hook(harness: Option<&str>, global: &Global, wiring: &Wiring) -> Option<Vec<u8>> {
    // A member never runs this hook; the plugin is also switched off for it.
    if std::env::var_os("TEAMWIRE_TEAM").is_some() {
        return None;
    }
    let home = state_home(global)?;
    lead_stop_in(
        harness,
        &home,
        wiring,
        &mut std::io::stdin().lock().take(STDIN_LIMIT),
    )
}

fn lead_stop_in(
    harness: Option<&str>,
    home: &Path,
    wiring: &Wiring,
    stdin: &mut dyn Read,
) -> Option<Vec<u8>> {
    let mut input = Vec::new();
    stdin.read_to_end(&mut input).ok()?;
    let id = match harness {
        Some(id) => HarnessId::parse(id).ok()?,
        None => claude_harness_id(),
    };
    let harness = wiring.registry.harness(&id).ok()?;
    let turn = harness.parse_turn_end(&input).ok()?;
    if turn.subagent || turn.hold_already_active {
        return None;
    }
    let platform = wiring.platform.clone();
    let clock = wiring.clock;
    let at = home.to_path_buf();
    let teams = within(LEAD_STOP_BUDGET, move || {
        teamwire_report::teams_without_watch(&at, &turn.session, &*platform, &clock)
    })?;
    if teams.is_empty() {
        return None;
    }
    let note: Vec<String> = teams
        .iter()
        .map(|team| {
            teamwire_report::watch_missing_note(team, lead_reads_watch_lines(home, team, wiring))
        })
        .collect();
    Some(harness.hold_reply(&note.join("\n")))
}

/// Whether the team's recorded lead harness reads a command's output as it is
/// printed. A team that cannot be read gets the Monitor wording, which is the
/// older one.
fn lead_reads_watch_lines(home: &Path, team: &TeamName, wiring: &Wiring) -> bool {
    Store::open_read_only(home, team)
        .ok()
        .and_then(|store| store.read(|tx| tx.team()).ok().flatten())
        .and_then(|row| wiring.registry.harness(&row.lead_harness).ok())
        .is_none_or(|harness| harness.capabilities().reads_watch_lines)
}

/// Runs `work` on a thread and gives up after `budget`. The thread is left
/// behind when the budget runs out; the process exits right after, which ends
/// it.
fn within<T: Send + 'static>(
    budget: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        tx.send(work()).ok();
    });
    rx.recv_timeout(budget).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_that_outlasts_the_budget_is_given_up_on() {
        // The work waits for a message that the test never sends, so only the
        // budget can end the wait.
        let (_keep, never) = mpsc::channel::<()>();
        let got = within(Duration::from_millis(50), move || never.recv().ok());
        assert!(got.is_none());
    }

    #[test]
    fn work_inside_the_budget_returns_its_value() {
        assert_eq!(within(Duration::from_secs(10), || 7), Some(7));
    }
}

#[cfg(test)]
mod member_tests;
