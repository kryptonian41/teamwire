//! `teamwire launch`: validate a team spec, then start its members.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, LaunchArgs};

pub(crate) fn run(args: &LaunchArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let LaunchArgs {
        add: _add,
        dry_run: _dry_run,
        spec: _spec,
    } = args;
    crate::not_built("launch", global, wiring, false)
}
