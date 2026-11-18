//! `teamwire dialog show` and `teamwire dialog answer` (ADR 0052): the lead
//! reads a member's dialog, and answers its permission dialog through the
//! member's own permission hook.
//!
//! Never: type into a pane, or send a message. A denial reaches the member
//! only in the hook's deny reply.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{DialogArgs, DialogCommand, Global};

mod answer;
mod show;

pub(crate) fn run(args: &DialogArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    match &args.command {
        DialogCommand::Show(a) => show::run(a, global, wiring),
        DialogCommand::Answer(a) => answer::run(a, global, wiring),
    }
}
