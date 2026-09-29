//! `teamwire export`: every record of a team as JSON lines.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{ExportArgs, Global};

pub(crate) fn run(args: &ExportArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ExportArgs {
        team: _team,
        out: _out,
    } = args;
    crate::not_built("export", global, wiring, false)
}
