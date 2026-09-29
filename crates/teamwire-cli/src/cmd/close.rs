//! `teamwire close`: close the team or remove one member.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{CloseArgs, Global};

pub(crate) fn run(args: &CloseArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let CloseArgs {
        team: _team,
        role: _role,
        stop: _stop,
        unverified: _unverified,
        dry_run: _dry_run,
        keep_state: _keep_state,
        archive_to: _archive_to,
        clear_stale_lock: _clear_stale_lock,
    } = args;
    crate::not_built("close", global, wiring, false)
}
