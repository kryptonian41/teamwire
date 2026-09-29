//! `teamwire hook`: the per-member Stop and `PostToolUse` hooks; always exits 0.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, HookArgs};

pub(crate) fn run(args: &HookArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let HookArgs { hook: _hook } = args;
    crate::not_built("hook", global, wiring, true)
}
