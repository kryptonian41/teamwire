//! `teamwire hook`: the per-member Stop and `PostToolUse` hooks, and the lead's
//! Stop and tool hooks (`hook/lead.rs`); always exits 0.

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{Config, Harness, MemberState, Role, TeamName, TurnEnd, TurnEndReply};
use teamwire_msg::CallerEnv;
use teamwire_store::{Ctx, Store};

use crate::cmd::shared::{caller_env, state_home, warn};
use crate::recorded::Recorded;
use crate::wiring::Wiring;
use crate::{Global, HookArgs, HookCommand, HookTarget};

mod lead;

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
        HookCommand::ClaudeLeadStop => lead::lead_stop_hook(None, global, wiring),
        HookCommand::LeadStop(a) => lead::lead_stop_hook(a.harness.as_deref(), global, wiring),
        HookCommand::LeadTool(a) => lead::lead_tool_hook(a.harness.as_deref(), global, wiring),
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
    let CallerEnv {
        team: team_var,
        role: role_var,
        token,
        ..
    } = caller_env(None);
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
    // From here on the session is a member's, so every way out says why on
    // stderr: a hook that does nothing must not look like one that worked.
    let team = said(TeamName::parse(&team_var), "the team name is not valid")?;
    let role = said(Role::parse(&role_var), "the role name is not valid")?;
    let Some(home) = state_home(global) else {
        return quit("no state directory: set --home, TEAMWIRE_HOME or HOME");
    };
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
    let opened = said(
        Store::open_existing(home, team),
        "the store could not be opened",
    )?;
    let mut store = Recorded::new(opened, &wiring.clock, kind.op());
    let found = said(
        // The token is checked even when absent, so every hook reads the
        // same rows and a damaged one fails each the same way.
        store.read(|tx| {
            Ok((
                tx.member(role)?,
                tx.token_matches(role, token.unwrap_or_default())?,
            ))
        }),
        "the member could not be read",
    )?;
    let (Some(member), token_matches) = found else {
        return quit(&format!("team {team} has no member {role}"));
    };
    if member.state == MemberState::Removed {
        return quit(&format!("member {role} was removed"));
    }
    let harness = said(
        wiring.registry.harness(&member.harness),
        "the member's harness is not in this binary",
    )?;
    if kind == MemberHook::PostTool && !harness.capabilities().in_turn_hook {
        return quit(&format!("harness {} has no in-turn hook", member.harness));
    }
    let mut input = Vec::new();
    said(
        stdin.read_to_end(&mut input),
        "standard input could not be read",
    )?;
    let turn = said(
        harness.parse_turn_end(&input),
        "the hook input did not parse",
    )?;

    if kind == MemberHook::SessionStart {
        // Only the member's own token may name its session (ADR 0023 point 5).
        match (token, token_matches) {
            (Some(_), Some(true)) => {}
            (Some(_), Some(false)) => return quit("the token does not belong to this member"),
            _ => return quit("no member token to prove the session with"),
        }
        said(
            store.write(|tx| tx.set_member_session(role, &turn.session)),
            "could not record the session",
        )?;
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
        post_tool_reply(&mut ctx, role, &turn, harness)
    } else {
        match teamwire_report::on_turn_end(&mut ctx, role, &turn) {
            TurnEndReply::Continue => None,
            TurnEndReply::Hold(note) => Some(harness.hold_reply(&note)),
        }
    }
}

impl MemberHook {
    /// The `op` of a `store.error` row this hook writes.
    fn op(self) -> &'static str {
        match self {
            Self::TurnEnd => "turn_end",
            Self::PostTool => "post_tool",
            Self::SessionStart => "session_start",
        }
    }
}

/// The member hook gives up: one stderr line says why, and the turn goes on.
fn quit<T>(why: &str) -> Option<T> {
    warn(&format!("teamwire hook: {why}"));
    None
}

/// `result`'s value, or `quit` with what failed and the error.
fn said<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => quit(&format!("{what}: {e}")),
    }
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

/// The in-turn reminder and handoff note, for the member's own session only.
fn post_tool_reply(
    ctx: &mut Ctx<'_>,
    role: &Role,
    turn: &TurnEnd,
    harness: &dyn Harness,
) -> Option<Vec<u8>> {
    if turn.subagent {
        return None;
    }
    let member = ctx.store.read(|tx| tx.member(role)).ok().flatten()?;
    if member.session_id.as_ref() != Some(&turn.session) {
        return None;
    }
    // The unread reminder and the handoff note go out in one reply.
    let note = [
        teamwire_report::on_tool_use(ctx, role),
        teamwire_report::handoff_note(
            ctx,
            role,
            harness,
            &turn.session,
            turn.transcript.as_deref(),
        ),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if note.is_empty() {
        return None;
    }
    tool_note_reply(&note.join("\n"))
}

/// The in-turn reply that hands `note` to the model beside a tool's result.
/// Every harness's in-turn hook reads this shape (ADR 0023 point 6), and so
/// do teamwire's pi extensions.
fn tool_note_reply(note: &str) -> Option<Vec<u8>> {
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
