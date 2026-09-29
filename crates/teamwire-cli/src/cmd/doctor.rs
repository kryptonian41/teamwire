//! `teamwire doctor`: read-only checks of state, disk and tools.

use std::process::ExitCode;

use crate::Global;
use crate::wiring::Wiring;

pub(crate) fn run(global: &Global, wiring: &Wiring) -> ExitCode {
    crate::not_built("doctor", global, wiring, false)
}
