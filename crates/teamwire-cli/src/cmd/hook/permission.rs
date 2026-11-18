//! `teamwire hook permission`: a member's `PermissionRequest` hook (ADR
//! 0052). It records the call as `dialog.opened`, waits for the lead's
//! `teamwire dialog answer`, and prints the member harness's allow or deny
//! reply for that one call.
//!
//! Never: type a key, print a lasting grant, or answer a dialog other than
//! the one it opened. With no answer it prints nothing, so the harness's own
//! time limit decides, as it would without teamwire.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use teamwire_core::{
    Clock as _, DialogState, MemberState, PERMISSION_WAIT, Role, TeamName, decision_for,
};
use teamwire_store::Store;

use super::{STDIN_LIMIT, quit, said, same_member};
use crate::cmd::shared::{caller_env, state_home};
use crate::recorded::Recorded;
use crate::wiring::Wiring;
use crate::{Global, HookTarget};

/// How often the hook reads the dialog while it waits for the lead.
const POLL: Duration = Duration::from_millis(250);

/// The bytes to print, or `None` to leave the dialog to the harness.
pub(super) fn permission_hook(
    target: &HookTarget,
    global: &Global,
    wiring: &Wiring,
) -> Option<Vec<u8>> {
    let env = caller_env(None);
    let (team_var, role_var) = same_member(target, env.team, env.role)?;
    let team = said(TeamName::parse(&team_var), "the team name is not valid")?;
    let role = said(Role::parse(&role_var), "the role name is not valid")?;
    let Some(home) = state_home(global) else {
        return quit("no state directory: set --home, TEAMWIRE_HOME or HOME");
    };
    let mut stdin = std::io::stdin().lock().take(STDIN_LIMIT);
    permission_hook_in(
        &home,
        &team,
        &role,
        env.token.as_deref(),
        wiring,
        &mut stdin,
        PERMISSION_WAIT,
    )
}

/// The hook's work after the environment check. `wait` is how long it waits
/// for the lead; production passes `PERMISSION_WAIT`.
pub(super) fn permission_hook_in(
    home: &Path,
    team: &TeamName,
    role: &Role,
    token: Option<&str>,
    wiring: &Wiring,
    stdin: &mut dyn Read,
    wait: Duration,
) -> Option<Vec<u8>> {
    let opened = said(
        Store::open_existing(home, team),
        "the store could not be opened",
    )?;
    let mut store = Recorded::new(opened, &wiring.clock, "permission");
    let found = said(
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
    // Only the member's own session may open a dialog in its name, so a
    // peer cannot plant a call for the lead to approve.
    if token.is_none() || token_matches != Some(true) {
        return quit("the token does not belong to this member");
    }
    let harness = said(
        wiring.registry.harness(&member.harness),
        "the member's harness is not in this binary",
    )?;
    let mut input = Vec::new();
    said(
        stdin.read_to_end(&mut input),
        "standard input could not be read",
    )?;
    let request = said(
        harness.parse_permission_request(&input),
        "the hook input did not parse",
    )?;
    if member.session_id.as_ref() != Some(&request.session) {
        return quit(&format!(
            "the call comes from session {}, not {role}'s current session",
            request.session
        ));
    }
    let now = wiring.clock.now();
    let expires_at = now.checked_add(wait).unwrap_or(now);
    let id = said(
        store.write(|tx| tx.open_dialog(now, role, member.generation, &request, expires_at)),
        "the dialog could not be recorded",
    )?;
    if let Some(why) = &request.why_not {
        return quit(&format!("dialog {id} is left to the harness: {why}"));
    }
    // Read transactions only, so the lead's answer can be written meanwhile.
    let deadline = wiring.clock.monotonic() + wait;
    while wiring.clock.monotonic() < deadline {
        let state = store
            .read(|tx| tx.dialog(&id, wiring.clock.now()))
            .ok()
            .flatten()
            .map(|d| d.state);
        if state == Some(DialogState::Answered) {
            break;
        }
        std::thread::sleep(POLL);
    }
    let answer = said(
        store.write(|tx| tx.close_dialog(wiring.clock.now(), &id)),
        "the dialog could not be closed",
    )?;
    let Some(answer) = answer else {
        return quit(&format!("nobody answered dialog {id}"));
    };
    said(
        harness.permission_reply(&decision_for(&id, &request, &answer)),
        "the harness could not write the reply",
    )
}

#[cfg(test)]
#[path = "permission_tests.rs"]
mod tests;
