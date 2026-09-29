//! `teamwire done`: record a member's completion.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{DoneArgs, Global};

pub(crate) fn run(args: &DoneArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let DoneArgs {
        status: _status,
        summary: _summary,
    } = args;
    crate::not_built("done", global, wiring, false)
}
