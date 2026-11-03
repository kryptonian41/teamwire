//! What a handoff needs from close: the busy and uncommitted-work checks of
//! a single removal, and the close of the member's place with its wait for
//! the place's processes to end. Launch's `hand_off` runs these, passed in by
//! the CLI, before it opens the new session, because the host refuses a
//! second live agent of one name.
//!
//! Nothing is removed: the row, worktree, branch and per-run files stay, so
//! a handoff needs no verify record and does not wait for the closer.

use teamwire_core::{Member, TeamName};
use teamwire_store::Ctx;

use crate::facts::{busy_state, busy_text, uncommitted_text};
use crate::member_close;

/// Why the member's place must not close for a handoff now: a running turn,
/// a dialog, a state nobody can read, or uncommitted work in its worktree.
/// Empty when it may. A handoff never stops a turn, so nothing overrides
/// these. A session that has already ended reads as idle.
pub fn handoff_refusals(ctx: &Ctx<'_>, member: &Member) -> Vec<String> {
    let busy = busy_text(
        &member.role,
        busy_state(ctx, member),
        "Wait for the turn to end, then run handoff again",
        "Answer it, then run handoff again",
    );
    let dirty = uncommitted_text(ctx, member, "The member commits its work before a handoff");
    busy.into_iter().chain(dirty).collect()
}

/// Closes the member's place through its own host and waits for the
/// processes that ran there to end, as `close --role` does before it removes
/// a worktree. A handoff waits in every mode: the host refuses a second live
/// agent of one name. `Ok` names what happened, `None` when no place is
/// recorded; an error means the old session may still run, so no new one may
/// start.
pub fn close_handoff_place(
    ctx: &Ctx<'_>,
    team: &TeamName,
    member: &Member,
) -> Result<Option<String>, String> {
    member_close::close_place_then(
        ctx,
        team,
        member,
        Some("no new session opens while they run"),
    )
}
