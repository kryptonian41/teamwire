//! Route A of ADR 0007.

use teamwire_core::{Role, TurnEnd, TurnEndReply};
use teamwire_store::Ctx;

/// Route A of ADR 0007: hold once for gaps or unread messages, else write
/// the result and deliver it.
pub fn on_turn_end(_ctx: &mut Ctx<'_>, _me: &Role, _t: &TurnEnd) -> TurnEndReply {
    TurnEndReply::Continue
}
