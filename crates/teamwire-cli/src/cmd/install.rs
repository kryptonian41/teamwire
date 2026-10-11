//! `teamwire install` and `teamwire uninstall`: write or remove the lead
//! setup in Claude Code's user configuration (ADR 0012). The only commands
//! that write there.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cmd::lead_setup_check::is_build_output;
use crate::cmd::send::{REFUSED, fail, state_home};
use crate::wiring::{ApplyError, Change, LeadSetup, McpEntry, Wiring};
use crate::{Global, InstallArgs, UninstallArgs};

/// A change failed after the checks passed (`docs/api/cli.md`).
const PARTIAL: u8 = 2;

/// The MCP entry this binary registers: its own absolute path, with
/// `--home` only when the state directory is not `~/.teamwire`.
pub(crate) fn lead_entry(bin: &Path, home: &Path, user_home: Option<&Path>) -> McpEntry {
    let default = user_home.map(|h| h.join(".teamwire"));
    let home_arg = (default.as_deref() != Some(home)).then_some(home);
    McpEntry::lead(bin, home_arg)
}

/// The running binary with symlinks resolved, so the entry names the file
/// that stays when a link to it is moved.
pub(crate) fn this_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    std::fs::canonicalize(&exe).map_err(|e| format!("cannot resolve {}: {e}", exe.display()))
}

/// The entry `doctor` and `install` compare against, or why there is none.
pub(crate) fn wanted_entry(global: &Global) -> Result<McpEntry, String> {
    let home = state_home(global)
        .ok_or_else(|| "no state directory: set --home, TEAMWIRE_HOME or HOME".to_owned())?;
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    Ok(lead_entry(&this_binary()?, &home, user_home.as_deref()))
}

#[expect(
    clippy::print_stdout,
    reason = "the change list is the command's output"
)]
fn say(text: &str) {
    println!("{text}");
}

pub(crate) fn run(args: &InstallArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let want = match wanted_entry(global) {
        Ok(want) => want,
        Err(e) => return fail("install", &e, REFUSED),
    };
    if is_build_output(Path::new(&want.command)) && !args.force {
        return fail(
            "install",
            &format!(
                "{} is a build inside a target/ directory; install the binary with \
                 `cargo install --path crates/teamwire-cli` and run that one, or pass --force",
                want.command
            ),
            REFUSED,
        );
    }
    let claude = wiring.lead_setup();
    match claude.plan_install(&want, args.force) {
        Ok(changes) if changes.is_empty() => {
            say("the lead setup is already installed");
            ExitCode::SUCCESS
        }
        Ok(changes) => finish("install", &claude, &changes, args.dry_run),
        Err(e) => fail("install", &e.to_string(), REFUSED),
    }
}

pub(crate) fn run_uninstall(args: &UninstallArgs, _global: &Global, wiring: &Wiring) -> ExitCode {
    let claude = wiring.lead_setup();
    match claude.plan_uninstall(args.force) {
        Ok(changes) if changes.is_empty() => {
            say("no lead setup to remove");
            ExitCode::SUCCESS
        }
        Ok(changes) => finish("uninstall", &claude, &changes, args.dry_run),
        Err(e) => fail("uninstall", &e.to_string(), REFUSED),
    }
}

/// Prints the plan for a dry run; otherwise makes the changes and prints
/// each one, or what was left behind when one fails.
fn finish(command: &str, claude: &LeadSetup, changes: &[Change], dry_run: bool) -> ExitCode {
    if dry_run {
        for change in changes {
            say(&format!("would {change}"));
        }
        return ExitCode::SUCCESS;
    }
    match claude.apply_lead_setup(changes) {
        Ok(()) => {
            for change in changes {
                say(&format!("did {change}"));
            }
            if changes.iter().any(Change::is_mcp) {
                say(
                    "Claude Code reads MCP servers at start: new sessions see the change, open ones do not",
                );
            }
            ExitCode::SUCCESS
        }
        // Exit 2 says some change was made. A failure with nothing changed is a
        // plain refusal, so a script does not read it as a half-done setup.
        Err(e) => {
            let code = if e.done.is_empty() && !e.entry_removed {
                REFUSED
            } else {
                PARTIAL
            };
            fail(command, &partial_text(command, &e), code)
        }
    }
}

/// What a failure partway through left behind, and how to finish.
fn partial_text(command: &str, e: &ApplyError) -> String {
    let mut lines = vec![e.to_string()];
    lines.extend(e.done.iter().map(|c| format!("already done: {c}")));
    if e.entry_removed {
        lines.push("the old MCP server entry was removed and the new one was not added".to_owned());
    }
    lines.push(format!(
        "run `teamwire {command}` again; it skips what is already done"
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_is_named_only_when_it_is_not_the_default() {
        let bin = Path::new("/u/.cargo/bin/teamwire");
        let user = Some(Path::new("/u"));
        assert_eq!(
            lead_entry(bin, Path::new("/u/.teamwire"), user).args,
            ["mcp", "--lead"]
        );
        assert_eq!(
            lead_entry(bin, Path::new("/scratch/tw"), user).args,
            ["--home", "/scratch/tw", "mcp", "--lead"]
        );
        assert_eq!(
            lead_entry(bin, Path::new("/u/.teamwire"), None).args,
            ["--home", "/u/.teamwire", "mcp", "--lead"]
        );
    }

    #[test]
    fn a_binary_under_target_is_a_build() {
        assert!(is_build_output(Path::new("/r/target/release/teamwire")));
        assert!(!is_build_output(Path::new("/u/.cargo/bin/teamwire")));
        assert!(!is_build_output(Path::new("/u/targets/teamwire")));
    }
}
