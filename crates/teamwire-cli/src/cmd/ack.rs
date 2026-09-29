//! `teamwire ack`: mark messages read without printing them.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{AckArgs, Global};

pub(crate) fn run(args: &AckArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let AckArgs { ids: _ids } = args;
    crate::not_built("ack", global, wiring, false)
}
