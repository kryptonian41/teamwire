//! `teamwire resend`: retry one stored message.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, ResendArgs};

pub(crate) fn run(args: &ResendArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ResendArgs {
        team: _team,
        id: _id,
        to: _to,
    } = args;
    crate::not_built("resend", global, wiring, false)
}
