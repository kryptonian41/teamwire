//! `teamwire version`: the binary's version and its schema version.

use std::process::ExitCode;

use crate::Global;
use crate::wiring::Wiring;

pub(crate) fn run(global: &Global, wiring: &Wiring) -> ExitCode {
    crate::not_built("version", global, wiring, false)
}
