//! `teamwire log`: every message with its delivery state.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, LogArgs};

pub(crate) fn run(args: &LogArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let LogArgs {
        team: _team,
        role: _role,
        json: _json,
    } = args;
    crate::not_built("log", global, wiring, false)
}
