//! `teamwire pr-watch`: watch GitHub pull-request checks (EXT-1).

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, PrWatchArgs};

pub(crate) fn run(args: &PrWatchArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let PrWatchArgs {
        repo: _repo,
        interval: _interval,
        prs: _prs,
    } = args;
    crate::not_built("pr-watch", global, wiring, false)
}
