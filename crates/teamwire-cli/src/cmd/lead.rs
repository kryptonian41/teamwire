//! `teamwire lead --adopt`: record this session as the team's lead.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, LeadArgs};

pub(crate) fn run(args: &LeadArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let LeadArgs { adopt: _adopt } = args;
    crate::not_built("lead --adopt", global, wiring, false)
}
