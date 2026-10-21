//! How full a member's context window is, as `status` and the watch read it.
//! One bounded read of the harness per live member per poll; an error or a
//! missing reading is "unknown" and never fails the caller.

use teamwire_core::Member;
use teamwire_core::MemberState;
use teamwire_store::{Ctx, Party};

use crate::ContextView;

/// The fill of the member's recorded session, or `None` when it has no live
/// session or the harness gives no reading.
pub(crate) fn read(ctx: &Ctx<'_>, member: &Member, configured: Option<u64>) -> Option<ContextView> {
    if member.state != MemberState::Started {
        return None;
    }
    let session = member.session_id.as_ref()?;
    let (harness, _) = ctx.adapters(&Party::Member(member.role.clone())).ok()?;
    let fill = harness.context_fill(session, None).ok()??;
    Some(ContextView {
        tokens: fill.tokens,
        window: fill.window,
        pct: fill.pct(),
        handoff_at: fill.handoff_at(configured),
    })
}
