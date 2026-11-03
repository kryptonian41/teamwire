//! The body of the `fake-herdr` binary: herdr's JSON command line for the
//! commands teamwire uses, over a JSON state file. See `tests/fakes/README.md`.
//!
//! Environment: `FAKE_HERDR_STATE` (required) names the state file;
//! `FAKE_HERDR_LOG` names a file that gets one JSON line per call;
//! `FAKE_HERDR_FAULTS` names a fault file (see `FaultRule`).

mod agent;
mod args;
mod faults;
mod layout;
mod pane_files;
mod shapes;
mod state;
mod syntax;

pub use faults::{Action, ErrorBody, Rule};

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};

use crate::fsutil;
use args::Args;
use state::State;

/// One error envelope: herdr prints `{"error":{"code":..,"message":..}}`
/// on stderr and exits 1.
#[derive(Debug)]
pub(crate) struct CliError {
    code: String,
    message: String,
}

pub(crate) fn fail(code: &str, message: impl Into<String>) -> CliError {
    CliError {
        code: code.to_owned(),
        message: message.into(),
    }
}

/// What a command works on.
#[derive(Debug)]
pub(crate) struct Ctx<'a> {
    pub(crate) state_path: &'a Path,
    pub(crate) state: &'a mut State,
    /// The state lock. A command that waits on something slow gives it up
    /// with `release_lock` and takes it again with `relock`.
    pub(crate) guard: &'a mut Option<state::Lock>,
}

impl Ctx<'_> {
    /// Saves the state and lets other calls run.
    pub(crate) fn release_lock(&mut self) -> Result<(), CliError> {
        state::save(self.state_path, self.state).map_err(|e| fail("fake_misconfigured", e))?;
        self.guard.take();
        Ok(())
    }

    /// Takes the lock again and reloads the state other calls changed.
    pub(crate) fn relock(&mut self) -> Result<(), CliError> {
        *self.guard =
            Some(state::lock(self.state_path).map_err(|e| fail("fake_misconfigured", e))?);
        *self.state = state::load(self.state_path).map_err(|e| fail("fake_misconfigured", e))?;
        Ok(())
    }
}

fn say(out: &mut impl std::io::Write, text: &str) {
    // A closed pipe is the caller's business; there is nowhere to report it.
    writeln!(out, "{text}").ok();
}

fn print_ok(command: &str, mut result: Value) {
    let id = format!("cli:{}", command.replace(' ', ":"));
    if result.is_null() {
        result = json!({});
    }
    say(
        &mut std::io::stdout().lock(),
        &json!({ "id": id, "result": result }).to_string(),
    );
}

fn print_err(command: &str, e: &CliError) {
    let id = format!("cli:{}", command.replace(' ', ":"));
    let body = json!({ "error": { "code": e.code, "message": e.message }, "id": id });
    say(&mut std::io::stderr().lock(), &body.to_string());
}

/// Read calls change nothing; every other call may.
fn is_read(command: &str) -> bool {
    matches!(
        command,
        "agent get"
            | "agent list"
            | "agent read"
            | "pane list"
            | "pane get"
            | "workspace get"
            | "workspace list"
    )
}

/// The file in a pane directory that holds the process group of the shell
/// step `fake-claude` is running, if it runs one.
pub(crate) const STEP_FILE: &str = "step.pgid";

/// Ends the agent process of a pane and the shell step it is running. Does
/// nothing for an agent that exited, whose pid may belong to someone else.
pub(crate) fn end_agent(state_path: &Path, pane_id: &str, pid: u32) {
    if agent_exited(state_path, pane_id) {
        return;
    }
    fsutil::terminate(pid);
    let dir = state::pane_dir(state_path, pane_id);
    let step = fsutil::read_text(&dir.join(STEP_FILE)).and_then(|t| t.trim().parse().ok());
    if let Some(step) = step {
        fsutil::terminate(step);
    }
}

/// True when the pane's agent wrote `exited`. Its recorded pid may belong to
/// an unrelated process by now, so nothing signals it.
pub(crate) fn agent_exited(state_path: &Path, pane_id: &str) -> bool {
    pane_files::read_status(&state::pane_dir(state_path, pane_id)).0 == "exited"
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn log_call(argv: &[String], command: &str, fault: Option<&Action>) {
    let Some(path) = env_path("FAKE_HERDR_LOG") else {
        return;
    };
    let fault = fault.map(|a| match a {
        Action::Error(e) => format!("error:{}", e.code),
        Action::Hang => "hang".to_owned(),
        Action::Garbage => "garbage".to_owned(),
        Action::Stalled => "stalled".to_owned(),
        Action::TimedOut => "timed_out".to_owned(),
    });
    let line = json!({
        "argv": argv,
        "kind": if is_read(command) { "read" } else { "write" },
        "fault": fault,
    });
    fsutil::append(&path, format!("{line}\n").as_bytes()).ok();
}

fn dispatch(ctx: &mut Ctx<'_>, args: &Args, fault: Option<&Action>) -> Result<Value, CliError> {
    match args.command().as_str() {
        "agent start" => agent::start(ctx, args),
        "agent prompt" => agent::prompt(ctx, args, fault),
        "agent get" => agent::get(ctx, args),
        "agent list" => Ok(agent::list(ctx)),
        "agent read" => agent::read(ctx, args),
        "agent send-keys" => agent::send_keys(ctx, args),
        "pane close" => layout::pane_close(ctx, args),
        "pane list" => Ok(layout::pane_list(ctx, args)),
        "pane get" => layout::pane_get(ctx, args),
        "workspace create" => Ok(layout::workspace_create(ctx, args)),
        "workspace get" => layout::workspace_get(ctx, args),
        "workspace rename" => layout::workspace_rename(ctx, args),
        "tab create" => layout::tab_create(ctx, args),
        "tab rename" => layout::tab_rename(ctx, args),
        "workspace list" => Ok(layout::workspace_list(ctx)),
        "pane rename" => layout::pane_rename(ctx, args),
        "fake add-unnamed-agent" => Ok(agent::add_unnamed_agent(ctx)),
        other => Err(fail("unknown_command", format!("no command `{other}`"))),
    }
}

/// Runs one call. The exit code is 0 for a result, 1 for an error envelope,
/// and 2 when the fake itself is misconfigured.
pub fn run(argv: &[String]) -> ExitCode {
    if argv == ["--version"] {
        say(&mut std::io::stdout().lock(), "herdr 0.0.0 (fake-herdr)");
        return ExitCode::SUCCESS;
    }
    let args = Args::parse(argv);
    let command = args.command();
    if let Err(text) = syntax::check(&args) {
        say(&mut std::io::stderr().lock(), &text);
        return ExitCode::from(2);
    }
    let Some(state_path) = env_path("FAKE_HERDR_STATE") else {
        print_err(
            &command,
            &fail("fake_misconfigured", "FAKE_HERDR_STATE is not set"),
        );
        return ExitCode::from(2);
    };
    let mut guard = match state::lock(&state_path) {
        Ok(l) => Some(l),
        Err(e) => {
            print_err(&command, &fail("fake_misconfigured", e));
            return ExitCode::from(2);
        }
    };
    let mut state = match state::load(&state_path) {
        Ok(s) => s,
        Err(e) => {
            print_err(&command, &fail("fake_misconfigured", e));
            return ExitCode::from(2);
        }
    };
    let fault_file = match env_path("FAKE_HERDR_FAULTS").map(|p| faults::load(&p)) {
        Some(Err(e)) => {
            print_err(&command, &fail("fake_misconfigured", e));
            return ExitCode::from(2);
        }
        other => other.and_then(Result::ok),
    };
    let fault = fault_file
        .as_ref()
        .and_then(|f| faults::take(f, &mut state, &command, args.operand(0)));
    log_call(argv, &command, fault.as_ref());

    match &fault {
        Some(Action::Hang) => {
            state::save(&state_path, &state).ok();
            guard.take();
            let ms = std::env::var("FAKE_HERDR_HANG_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30_000);
            std::thread::sleep(Duration::from_millis(ms));
            print_err(&command, &fail("timeout", "fake-herdr hung and gave up"));
            return ExitCode::from(1);
        }
        Some(Action::Garbage) => {
            state::save(&state_path, &state).ok();
            say(
                &mut std::io::stdout().lock(),
                "<<not json: fake-herdr garbage>> {\"id\":",
            );
            return ExitCode::SUCCESS;
        }
        Some(Action::Error(e)) => {
            state::save(&state_path, &state).ok();
            print_err(&command, &fail(&e.code, e.message.clone()));
            return ExitCode::from(1);
        }
        Some(Action::Stalled | Action::TimedOut) | None => {}
    }

    let mut ctx = Ctx {
        state_path: &state_path,
        state: &mut state,
        guard: &mut guard,
    };
    let outcome = dispatch(&mut ctx, &args, fault.as_ref());
    if let Err(e) = state::save(&state_path, &state) {
        print_err(&command, &fail("fake_misconfigured", e));
        return ExitCode::from(2);
    }
    match outcome {
        Ok(Value::String(screen)) if command == "agent read" => {
            // herdr 0.9.0 prints the screen with no envelope and no final newline.
            std::io::Write::write_all(&mut std::io::stdout().lock(), screen.as_bytes()).ok();
            ExitCode::SUCCESS
        }
        Ok(result) => {
            print_ok(&command, result);
            ExitCode::SUCCESS
        }
        Err(e) => {
            print_err(&command, &e);
            ExitCode::from(1)
        }
    }
}
