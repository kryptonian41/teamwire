//! `teamwire speckit draft`: reads a spec-kit `tasks.md` and prints the
//! files phases and stories share, the tasks that can run apart, and a DRAFT
//! `owns` list per story (ADR 0044).
//!
//! Never: write a file or a store row, or call the network. Git is asked
//! only for the tracked files. The lists are drafts for the lead to check.

use std::process::ExitCode;

use crate::cmd::shared::{REFUSED, fail};
use crate::wiring::Wiring;
use crate::{Global, SpeckitArgs, SpeckitCommand, SpeckitDraftArgs};

// Paths, as `doctor_speckit.rs` attaches its parts.
#[path = "speckit/mentions.rs"]
mod mentions;
#[path = "speckit/overlap.rs"]
mod overlap;
#[path = "speckit/parse.rs"]
mod parse;
#[path = "speckit/render.rs"]
mod render;

pub(crate) fn run(args: &SpeckitArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    match &args.command {
        SpeckitCommand::Draft(a) => draft(a, global, wiring),
    }
}

const COMMAND: &str = "speckit draft";

fn draft(args: &SpeckitDraftArgs, _global: &Global, _wiring: &Wiring) -> ExitCode {
    let shown = args.tasks.display();
    let text = match std::fs::read_to_string(&args.tasks) {
        Ok(text) => text,
        Err(e) => return fail(COMMAND, &format!("cannot read {shown}: {e}"), REFUSED),
    };
    let list = parse::parse(&text);
    if list.tasks.is_empty() {
        return fail(
            COMMAND,
            &format!("{shown} has no task lines (- [ ] T<n> ...)"),
            REFUSED,
        );
    }
    ExitCode::SUCCESS
}
