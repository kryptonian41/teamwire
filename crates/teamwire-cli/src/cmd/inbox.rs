//! `teamwire inbox`: print unread messages and mark them read.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, InboxArgs};

pub(crate) fn run(args: &InboxArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let InboxArgs {
        all: _all,
        json: _json,
    } = args;
    crate::not_built("inbox", global, wiring, false)
}
