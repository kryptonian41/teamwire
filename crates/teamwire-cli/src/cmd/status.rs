//! `teamwire status`: the read-only team view.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, StatusArgs};

pub(crate) fn run(args: &StatusArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let StatusArgs {
        team: _team,
        role: _role,
        json: _json,
    } = args;
    crate::not_built("status", global, wiring, false)
}
