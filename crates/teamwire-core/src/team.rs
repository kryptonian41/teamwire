//! The team row's two enum columns.

use crate::strings::string_enum;

string_enum! {
    pub enum TeamState {
        Open => "open",
        Closing => "closing",
        Closed => "closed",
    }
}

string_enum! {
    /// How results reach the lead (ADR 0007): typed into its session by the
    /// member's hook, or printed by the watch.
    pub enum LeadRoute {
        Sink => "sink",
        Watch => "watch",
    }
}
