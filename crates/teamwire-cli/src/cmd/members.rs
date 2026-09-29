//! `teamwire members`: the team roster.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, MembersArgs};

pub(crate) fn run(args: &MembersArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let MembersArgs {
        team: _team,
        json: _json,
    } = args;
    crate::not_built("members", global, wiring, false)
}
