//! `teamwire install` and `teamwire uninstall`: register or remove the
//! teamwire plugin in Claude Code, and remove the skill and MCP entry that
//! ADR 0012 wrote (ADR 0018); add or remove the plugin in Codex (ADR 0023);
//! add or remove the lead package in pi (ADR 0030). The only commands that
//! change a harness's configuration, and only through `claude plugin`,
//! `claude mcp remove`, `codex plugin`, `pi install` and `pi remove`.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::Command;

use crate::cmd::lead_setup_check::is_build_output;
use crate::cmd::send::{PARTIAL, REFUSED, fail, say_line, state_home};
use crate::wiring::{
    Change, CodexChange, CodexCommand, McpEntry, PiChange, PiCommand, SKILL_TEXT, Wiring,
};
use crate::{Global, InstallArgs, USAGE, UninstallArgs};

/// The harness ids `install` knows, in the order a selector lists them, with
/// the program each one runs.
const CLAUDE: &str = "claude-code";
const CODEX: &str = "codex";
const PI: &str = "pi";

const NO_HOME: &str = "no state directory: set --home, TEAMWIRE_HOME or HOME";

/// The MCP entry this binary registers: its own absolute path, with
/// `--home` only when the state directory is not `~/.teamwire`.
pub(crate) fn lead_entry(bin: &Path, home: &Path, user_home: Option<&Path>) -> McpEntry {
    McpEntry::lead(bin, home_flag(home, user_home))
}

/// `home` when it is not the default state directory, so a default install
/// keeps following the default.
pub(crate) fn home_flag<'a>(home: &'a Path, user_home: Option<&Path>) -> Option<&'a Path> {
    let default = user_home.map(|h| h.join(".teamwire"));
    (default.as_deref() != Some(home)).then_some(home)
}

/// The command the Codex plugin runs: the same binary and `--home` as the
/// Claude entry.
pub(crate) fn codex_command(
    want: &McpEntry,
    home: &Path,
    user_home: Option<&Path>,
) -> CodexCommand {
    CodexCommand {
        bin: PathBuf::from(&want.command),
        home: home_flag(home, user_home).map(Path::to_path_buf),
    }
}

/// The command the pi package runs: the same binary and `--home` as the
/// Claude entry.
pub(crate) fn pi_command(want: &McpEntry, home: &Path, user_home: Option<&Path>) -> PiCommand {
    PiCommand {
        bin: PathBuf::from(&want.command),
        home: home_flag(home, user_home).map(Path::to_path_buf),
    }
}

/// The first line `<program> --version` prints, when the program runs.
pub(crate) fn version_of(wiring: &Wiring, program: &str) -> Option<String> {
    let out = wiring
        .platform
        .run(
            &Command::new(program).arg("--version"),
            Duration::from_secs(10),
        )
        .ok()
        .filter(teamwire_core::RunOutput::success)?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned(),
    )
}

/// The harnesses whose program runs, in selector order.
pub(crate) fn found(wiring: &Wiring) -> Vec<&'static str> {
    [
        (CLAUDE, &wiring.claude.claude_bin),
        (CODEX, &wiring.codex.codex_bin),
        (PI, &wiring.pi.pi_bin),
    ]
    .into_iter()
    .filter(|(_, program)| version_of(wiring, program).is_some())
    .map(|(id, _)| id)
    .collect()
}

/// Every harness whose program answers `--version`. Launch refuses a member
/// whose harness is not among them.
pub(crate) fn on_path(wiring: &Wiring) -> Vec<&'static str> {
    found(wiring)
}

/// The harnesses a selector line picks from the numbered list `found`: an
/// empty line keeps them all, and `1,2` picks those numbers.
pub(crate) fn parse_selection(
    line: &str,
    found: &[&'static str],
) -> Result<Vec<&'static str>, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(found.to_vec());
    }
    let mut picked = Vec::new();
    for part in line.split(',').map(str::trim) {
        let id = part
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| found.get(i))
            .ok_or_else(|| format!("`{part}` is not a number from the list"))?;
        if !picked.contains(id) {
            picked.push(*id);
        }
    }
    Ok(picked)
}

#[expect(
    clippy::print_stdout,
    reason = "the selector is the command's interactive output"
)]
fn ask(found: &[&'static str]) -> Result<Vec<&'static str>, String> {
    println!("Install the lead setup for:");
    for (i, id) in found.iter().enumerate() {
        println!("  {}. {id}", i + 1);
    }
    println!("Press Enter for all, or type numbers such as 1,2:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("cannot read the answer: {e}"))?;
    parse_selection(&line, found)
}

/// The harnesses to install for: `--harness`, else the selector in a
/// terminal, else `claude-code` so a script keeps its behaviour. The error
/// carries its exit code: a harness that is not on PATH is a refusal, a name
/// or an answer that makes no sense is a usage error.
fn choose(args: &InstallArgs, wiring: &Wiring) -> Result<Vec<&'static str>, (String, u8)> {
    if args.harness.is_empty() {
        if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
            return Ok(vec![CLAUDE]);
        }
        let found = found(wiring);
        if found.is_empty() {
            return Err((
                "none of `claude`, `codex` and `pi` is on PATH".to_owned(),
                REFUSED,
            ));
        }
        return ask(&found).map_err(|e| (e + "; nothing was changed", USAGE));
    }
    let mut chosen = Vec::new();
    for name in &args.harness {
        let id = [CLAUDE, CODEX, PI]
            .into_iter()
            .find(|id| id == name)
            .ok_or_else(|| {
                (
                    format!("unknown harness `{name}`; choose claude-code, codex or pi"),
                    USAGE,
                )
            })?;
        if !chosen.contains(&id) {
            chosen.push(id);
        }
    }
    let found = found(wiring);
    match chosen.iter().find(|id| !found.contains(id)) {
        Some(id) => Err((format!("harness `{id}` is not on PATH"), REFUSED)),
        None => Ok(chosen),
    }
}

/// The running binary with symlinks resolved, so the entry names the file
/// that stays when a link to it is moved.
pub(crate) fn this_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    std::fs::canonicalize(&exe).map_err(|e| format!("cannot resolve {}: {e}", exe.display()))
}

/// The entry `doctor` and `install` compare against, or why there is none.
pub(crate) fn wanted_entry(global: &Global) -> Result<McpEntry, String> {
    let home = state_home(global).ok_or_else(|| NO_HOME.to_owned())?;
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    Ok(lead_entry(&this_binary()?, &home, user_home.as_deref()))
}

/// One harness's planned changes.
enum Plan {
    Claude(Vec<Change>),
    Codex(Vec<CodexChange>),
    Pi(Vec<PiChange>),
}

impl Plan {
    fn lines(&self) -> Vec<String> {
        match self {
            Self::Claude(c) => c.iter().map(ToString::to_string).collect(),
            Self::Codex(c) => c.iter().map(ToString::to_string).collect(),
            Self::Pi(c) => c.iter().map(ToString::to_string).collect(),
        }
    }
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
    let Some(home) = state_home(global) else {
        return fail("install", NO_HOME, REFUSED);
    };
    let chosen = match choose(args, wiring) {
        Ok(chosen) => chosen,
        Err((e, code)) => return fail("install", &e, code),
    };
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    // Every harness is planned before any is changed, so one refusal leaves
    // all of them as they were.
    let mut plans = Vec::new();
    for id in chosen {
        let planned = if id == CODEX {
            let cmd = codex_command(&want, &home, user_home.as_deref());
            wiring
                .codex_setup()
                .plan_install(&cmd, SKILL_TEXT, &home, args.force)
                .map(Plan::Codex)
                .map_err(|e| e.to_string())
        } else if id == PI {
            let cmd = pi_command(&want, &home, user_home.as_deref());
            wiring
                .pi_setup()
                .plan_install(&cmd, SKILL_TEXT, &home)
                .map(Plan::Pi)
                .map_err(|e| e.to_string())
        } else {
            wiring
                .lead_setup()
                .plan_install(&want, &home, args.force)
                .map(Plan::Claude)
                .map_err(|e| e.to_string())
        };
        match planned {
            Ok(plan) => plans.push(plan),
            Err(e) => return fail("install", &e, REFUSED),
        }
    }
    if plans.iter().all(|p| p.lines().is_empty()) {
        say_line("the lead setup is already installed");
        return ExitCode::SUCCESS;
    }
    finish("install", wiring, &plans, args.dry_run)
}

pub(crate) fn run_uninstall(args: &UninstallArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let Some(home) = state_home(global) else {
        return fail("uninstall", NO_HOME, REFUSED);
    };
    let mut plans = Vec::new();
    match wiring.lead_setup().plan_uninstall(&home, args.force) {
        Ok(changes) => plans.push(Plan::Claude(changes)),
        Err(e) => return fail("uninstall", &e.to_string(), REFUSED),
    }
    // Codex's setup can be removed only through `codex`, so a machine
    // without it has none to remove.
    if version_of(wiring, &wiring.codex.codex_bin).is_some() {
        match wiring.codex_setup().plan_uninstall(&home, args.force) {
            Ok(changes) => plans.push(Plan::Codex(changes)),
            // An old or broken `codex` must not keep the Claude setup in place.
            Err(e) => skipped("uninstall", "Codex", &e.to_string()),
        }
    }
    if version_of(wiring, &wiring.pi.pi_bin).is_some() {
        match wiring.pi_setup().plan_uninstall(&home) {
            Ok(changes) => plans.push(Plan::Pi(changes)),
            Err(e) => skipped("uninstall", "pi", &e.to_string()),
        }
    }
    if plans.iter().all(|p| p.lines().is_empty()) {
        say_line("no lead setup to remove");
        return ExitCode::SUCCESS;
    }
    finish("uninstall", wiring, &plans, args.dry_run)
}

#[expect(
    clippy::print_stderr,
    reason = "the skipped part is told on stderr, beside the other part's output"
)]
fn skipped(command: &str, part: &str, why: &str) {
    eprintln!("teamwire {command}: skipped the {part} part: {why}");
}

/// Prints the plan for a dry run; otherwise makes the changes and prints
/// each one, or what was left behind when one fails.
fn finish(command: &str, wiring: &Wiring, plans: &[Plan], dry_run: bool) -> ExitCode {
    if dry_run {
        for line in plans.iter().flat_map(Plan::lines) {
            say_line(&format!("would {line}"));
        }
        return ExitCode::SUCCESS;
    }
    let mut done = Vec::new();
    for plan in plans {
        let applied = match plan {
            Plan::Claude(changes) => wiring
                .lead_setup()
                .apply_lead_setup(changes)
                .map_err(|e| failure(&e, &e.done)),
            Plan::Codex(changes) => wiring
                .codex_setup()
                .apply_lead_setup(changes)
                .map_err(|e| failure(&e, &e.done)),
            Plan::Pi(changes) => wiring
                .pi_setup()
                .apply_lead_setup(changes)
                .map_err(|e| failure(&e, &e.done)),
        };
        if let Err((text, partial)) = applied {
            done.extend(partial);
            // Exit 2 says some change was made. A failure with nothing changed
            // is a plain refusal, so a script does not read it as a half-done
            // setup.
            let code = if done.is_empty() { REFUSED } else { PARTIAL };
            return fail(command, &partial_text(command, &text, &done), code);
        }
        done.extend(plan.lines());
        for line in plan.lines() {
            say_line(&format!("did {line}"));
        }
    }
    if plans
        .iter()
        .any(|p| matches!(p, Plan::Claude(c) if c.iter().any(Change::is_claude)))
    {
        say_line(
            "Claude Code loads plugins at start: new sessions see the change, open ones do not",
        );
    }
    if plans
        .iter()
        .any(|p| matches!(p, Plan::Codex(c) if c.iter().any(CodexChange::needs_hook_review)))
    {
        say_line("Codex will ask you to review the plugin's hook once after this change");
    }
    if plans
        .iter()
        .any(|p| matches!(p, Plan::Pi(c) if !c.is_empty()))
    {
        say_line("pi loads packages at start: new pi sessions see the change, open ones do not");
    }
    ExitCode::SUCCESS
}

/// The failure's text, and the lines of the changes made before it.
fn failure<E: ToString, C: ToString>(e: &E, done: &[C]) -> (String, Vec<String>) {
    (
        e.to_string(),
        done.iter().map(ToString::to_string).collect(),
    )
}

/// What a failure partway through left behind, and how to finish.
fn partial_text(command: &str, failed: &str, done: &[String]) -> String {
    let mut lines = vec![failed.to_owned()];
    lines.extend(done.iter().map(|c| format!("already done: {c}")));
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

    #[test]
    fn a_selector_line_keeps_all_or_picks_numbers() {
        let found = [CLAUDE, CODEX];
        assert_eq!(parse_selection("", &found).unwrap(), [CLAUDE, CODEX]);
        assert_eq!(parse_selection(" \n", &found).unwrap(), [CLAUDE, CODEX]);
        assert_eq!(parse_selection("2", &found).unwrap(), [CODEX]);
        assert_eq!(parse_selection("2, 1\n", &found).unwrap(), [CODEX, CLAUDE]);
        assert_eq!(parse_selection("1,1", &found).unwrap(), [CLAUDE]);
        for bad in ["0", "3", "x", "1,", "claude-code"] {
            assert!(parse_selection(bad, &found).is_err(), "{bad}");
        }
        // Numbers count the list that was shown, not both harnesses.
        assert_eq!(parse_selection("1", &[CODEX]).unwrap(), [CODEX]);
        assert!(parse_selection("2", &[CODEX]).is_err());
    }
}
