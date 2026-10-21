//! What a handoff needs from close: the busy and uncommitted-work checks of
//! a single removal, and the close of the member's place with its wait for
//! the place's processes to end. The CLI runs these before launch opens the
//! new session, because the host refuses a second live agent of one name.
//!
//! Nothing is removed: the row, worktree, branch and per-run files stay, so
//! a handoff needs no verify record and does not wait for the closer.

use teamwire_core::{Member, TeamName};
use teamwire_store::Ctx;

use crate::facts::{Busy, busy_state, dirty_paths};
use crate::member_close;

/// Why the member's place must not close for a handoff now: a running turn,
/// a dialog, a state nobody can read, or uncommitted work in its worktree.
/// Empty when it may. A handoff never stops a turn, so nothing overrides
/// these. A session that has already ended reads as idle.
pub fn handoff_refusals(ctx: &Ctx<'_>, member: &Member) -> Vec<String> {
    let role = &member.role;
    let mut out = Vec::new();
    match busy_state(ctx, member) {
        Busy::Idle => {}
        Busy::Working => out.push(format!(
            "{role} is busy: its current turn is lost if its place closes. Wait for the turn \
             to end, then run handoff again"
        )),
        Busy::Blocked => out.push(format!(
            "{role} is showing a dialog and waits for an answer. Answer it, then run handoff \
             again"
        )),
        Busy::Unknown(why) => out.push(format!(
            "cannot tell whether {role} is busy ({why}); refusing rather than guess"
        )),
    }
    match dirty_paths(ctx, member) {
        Ok(paths) if paths.is_empty() => {}
        Ok(paths) => out.push(format!(
            "{role} has {} uncommitted path(s) in {}. The member commits its work before a \
             handoff",
            paths.len(),
            member.cwd.display()
        )),
        Err(e) => out.push(format!(
            "cannot tell whether {role} has uncommitted work ({e}); refusing rather than guess"
        )),
    }
    out
}

/// Closes the member's place through its own host and waits for the
/// processes that ran there to end, as `close --role` does. `Ok` names what
/// happened, `None` when no place is recorded; an error means the old
/// session may still run, so no new one may start.
pub fn close_handoff_place(
    ctx: &Ctx<'_>,
    team: &TeamName,
    member: &Member,
) -> Result<Option<String>, String> {
    member_close::close_place(ctx, team, member)
}
