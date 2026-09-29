//! `teamwire verify`: each member's check and the diff rules, and waivers.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, VerifyArgs};

pub(crate) fn run(args: &VerifyArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let VerifyArgs {
        team: _team,
        role: _role,
        waive: _waive,
        reason: _reason,
        deep: _deep,
        repair_files: _repair_files,
    } = args;
    crate::not_built("verify", global, wiring, false)
}
