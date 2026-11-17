//! `pane run`: types a command line into a pane's shell. The fake starts the
//! line with `sh`, and records it as an unnamed `claude` agent of the pane,
//! as real herdr detects an agent that a user started by hand. `agent get
//! <pane>` then reports the session that `fake-claude` writes.

use std::path::Path;

use serde_json::Value;

use super::agent::spawn_in_pane;
use super::args::Args;
use super::pane_files::alive;
use super::state::{AgentRec, pane_dir};
use super::{CliError, Ctx, fail};

/// The longest command line the fake types. macOS refuses a terminal line of
/// 1,024 bytes or more (`getconf MAX_CANON /dev/tty`), and real herdr rejects
/// such a command before it queues it (`herdr --skill`). The error code
/// `command_too_long` is the fake's own: herdr's is not known.
const MAX_LINE: usize = 1023;

pub(super) fn pane_run(ctx: &mut Ctx<'_>, args: &Args) -> Result<Value, CliError> {
    let id = args.operand(0).unwrap_or_default().to_owned();
    let pane = ctx
        .state
        .pane(&id)
        .cloned()
        .ok_or_else(|| fail("pane_not_found", format!("pane {id} not found")))?;
    let line = args
        .words
        .iter()
        .skip(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if line.len() > MAX_LINE {
        return Err(fail(
            "command_too_long",
            format!(
                "the command is {} bytes, over the terminal's line limit",
                line.len()
            ),
        ));
    }
    if pane.agent.as_ref().is_some_and(alive) {
        return Err(fail(
            "fake_pane_occupied",
            "fake-herdr runs a command only in a pane whose program has exited",
        ));
    }
    let dir = pane_dir(ctx.state_path, &id);
    std::fs::remove_dir_all(&dir).ok();
    // `exec` keeps one process from the shell to the program, so the pid the
    // fake records is the pid `fake-claude` names its session file by.
    let argv = ["-c".to_owned(), format!("exec {line}")];
    let pid = spawn_in_pane(&dir, &pane, Path::new("sh"), &argv)?;
    let rec = AgentRec {
        name: String::new(),
        kind: "claude".to_owned(),
        pid,
        session_hint: None,
        fixed: false,
    };
    if let Some(p) = ctx.state.panes.iter_mut().find(|p| p.pane_id == id) {
        p.agent = Some(rec);
    }
    // Real herdr prints nothing for `pane run`; `run` prints no envelope.
    Ok(Value::Null)
}
