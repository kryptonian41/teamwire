//! `teamwire preset`: the six team shapes.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, PresetArgs};

pub(crate) fn run(args: &PresetArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let PresetArgs { name: _name } = args;
    crate::not_built("preset", global, wiring, false)
}
