//! The in-turn reminder of unread messages.

use teamwire_core::Role;
use teamwire_store::Ctx;

/// The reminder of unread message ids for the in-turn hook, once per id.
pub fn on_tool_use(_ctx: &mut Ctx<'_>, _me: &Role) -> Option<String> {
    None
}
