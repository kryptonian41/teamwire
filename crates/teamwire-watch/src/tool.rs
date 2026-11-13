//! The tool call a member's session runs now, as `status --role` reads it.
//! One bounded read of the harness for the one member `--role` names; an
//! error or a missing reading is "unknown" and never fails the caller.

use teamwire_core::{Member, MemberState, Timestamp};
use teamwire_store::{Ctx, Party};

use crate::ToolView;

/// The call the member's recorded session runs, and how long it has run at
/// `now`, or `None` when it has no live session or the harness cannot tell.
pub(crate) fn read(ctx: &Ctx<'_>, member: &Member, now: Timestamp) -> Option<ToolView> {
    if member.state != MemberState::Started {
        return None;
    }
    let session = member.session_id.as_ref()?;
    let (harness, _) = ctx.adapters(&Party::Member(member.role.clone())).ok()?;
    let run = harness.current_tool(session, Some(&member.cwd)).ok()??;
    Some(ToolView {
        seconds: now.saturating_since(run.started_at).as_secs(),
        name: run.name,
        started_at: run.started_at,
    })
}
