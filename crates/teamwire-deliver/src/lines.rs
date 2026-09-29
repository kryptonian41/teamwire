//! placeholder
use std::path::Path;
use teamwire_core::{
    Actor, CompletionStatus, DisplayId, MessageType, OneLine, Role, TeamName, one_line,
};

pub fn message_line(
    _t: &TeamName,
    _i: &DisplayId,
    _k: MessageType,
    _s: &Actor,
    _b: &str,
) -> OneLine {
    one_line("", 1)
}
pub fn result_line(_t: &TeamName, _r: &Role, _s: CompletionStatus, _m: &str, _p: &Path) -> OneLine {
    one_line("", 1)
}
