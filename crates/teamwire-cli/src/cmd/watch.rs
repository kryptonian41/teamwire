//! `teamwire watch`: one line per poll when something needs the lead.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, WatchArgs};

pub(crate) fn run(args: &WatchArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let WatchArgs {
        team: _team,
        interval: _interval,
        once: _once,
    } = args;
    crate::not_built("watch", global, wiring, false)
}
