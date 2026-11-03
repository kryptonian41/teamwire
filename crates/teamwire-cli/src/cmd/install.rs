//! `teamwire install` and `teamwire uninstall`: plan and make each
//! harness's lead setup through the `LeadSetup` seam (ADR 0031): the plugin
//! in Claude Code (ADR 0018), the plugin in Codex (ADR 0023), the lead
//! package in pi (ADR 0030). The only commands that change a harness's
//! configuration, and only through that harness's own program.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::{Command, LeadCommand, LeadPlan, LeadSetup, LeadTarget};

use crate::cmd::lead_setup_check::is_build_output;
use crate::cmd::shared::{PARTIAL, REFUSED, fail, say_line, state_home};
use crate::wiring::{SKILL_TEXT, Wiring};
use crate::{Global, InstallArgs, USAGE, UninstallArgs};

const NO_HOME: &str = "no state directory: set --home, TEAMWIRE_HOME or HOME";

/// The command every lead setup runs: this binary, with `--home` only when
/// the state directory is not `~/.teamwire`.
pub(crate) fn lead_command(bin: &Path, home: &Path, user_home: Option<&Path>) -> LeadCommand {
    LeadCommand {
        bin: bin.to_path_buf(),
        home: home_flag(home, user_home).map(Path::to_path_buf),
    }
}

/// `home` when it is not the default state directory, so a default install
/// keeps following the default.
pub(crate) fn home_flag<'a>(home: &'a Path, user_home: Option<&Path>) -> Option<&'a Path> {
    let default = user_home.map(|h| h.join(".teamwire"));
    (default.as_deref() != Some(home)).then_some(home)
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

/// The harnesses whose program runs, in selector order. Launch refuses a
/// member whose harness is not among them.
pub(crate) fn found(wiring: &Wiring) -> Vec<&str> {
    wiring
        .registry
        .lead_setups()
        .into_iter()
        .filter(|(_, setup)| version_of(wiring, setup.program()).is_some())
        .map(|(id, _)| id)
        .collect()
}

/// `a, b or c`: the words joined for a sentence, `last` before the last one.
fn sentence(words: &[String], last: &str) -> String {
    match words.split_last() {
        None => String::new(),
        Some((one, [])) => one.clone(),
        Some((tail, rest)) => format!("{} {last} {tail}", rest.join(", ")),
    }
}

/// The harnesses a selector line picks from the numbered list `found`: an
/// empty line keeps them all, and `1,2` picks those numbers.
pub(crate) fn parse_selection<'a>(line: &str, found: &[&'a str]) -> Result<Vec<&'a str>, String> {
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
fn ask<'a>(found: &[&'a str]) -> Result<Vec<&'a str>, String> {
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
/// terminal, else the default lead harness so a script keeps its behaviour.
/// The error carries its exit code: a harness that is not on PATH is a
/// refusal, a name or an answer that makes no sense is a usage error.
fn choose<'w>(args: &InstallArgs, wiring: &'w Wiring) -> Result<Vec<&'w str>, (String, u8)> {
    let setups = wiring.registry.lead_setups();
    if args.harness.is_empty() {
        if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
            return Ok(setups
                .iter()
                .filter(|(_, s)| s.default_lead())
                .map(|(id, _)| *id)
                .collect());
        }
        let found = found(wiring);
        if found.is_empty() {
            let programs: Vec<String> = setups
                .iter()
                .map(|(_, s)| format!("`{}`", s.program()))
                .collect();
            return Err((
                format!("none of {} is on PATH", sentence(&programs, "and")),
                REFUSED,
            ));
        }
        return ask(&found).map_err(|e| (e + "; nothing was changed", USAGE));
    }
    let mut chosen = Vec::new();
    for name in &args.harness {
        let id = setups
            .iter()
            .map(|(id, _)| *id)
            .find(|id| id == name)
            .ok_or_else(|| {
                let ids: Vec<String> = setups.iter().map(|(id, _)| (*id).to_owned()).collect();
                (
                    format!("unknown harness `{name}`; choose {}", sentence(&ids, "or")),
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

/// The command `doctor` and `install` compare against, or why there is none.
pub(crate) fn wanted_command(global: &Global) -> Result<LeadCommand, String> {
    let home = state_home(global).ok_or_else(|| NO_HOME.to_owned())?;
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    Ok(lead_command(&this_binary()?, &home, user_home.as_deref()))
}

/// One harness's lead setup and its planned changes.
type Planned<'w> = (&'w dyn LeadSetup, LeadPlan);

fn lines(plan: &LeadPlan) -> Vec<String> {
    plan.changes.iter().map(ToString::to_string).collect()
}

pub(crate) fn run(args: &InstallArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let want = match wanted_command(global) {
        Ok(want) => want,
        Err(e) => return fail("install", &e, REFUSED),
    };
    if is_build_output(&want.bin) && !args.force {
        return fail(
            "install",
            &format!(
                "{} is a build inside a target/ directory; install the binary with \
                 `cargo install --path crates/teamwire-cli` and run that one, or pass --force",
                want.bin.display()
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
    let target = LeadTarget {
        command: &want,
        home: &home,
        shared_skill: SKILL_TEXT,
        compare_files: true,
    };
    // Every harness is planned before any is changed, so one refusal leaves
    // all of them as they were.
    let setups = wiring.registry.lead_setups();
    let mut plans = Vec::new();
    for id in chosen {
        let Some(&(_, setup)) = setups.iter().find(|(known, _)| *known == id) else {
            continue;
        };
        match setup.plan_install(&target, args.force) {
            Ok(plan) => plans.push((setup, plan)),
            Err(e) => return fail("install", &e.to_string(), REFUSED),
        }
    }
    if plans.iter().all(|(_, p)| p.changes.is_empty()) {
        say_line("the lead setup is already installed");
        return ExitCode::SUCCESS;
    }
    finish("install", &plans, args.dry_run)
}

pub(crate) fn run_uninstall(args: &UninstallArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let Some(home) = state_home(global) else {
        return fail("uninstall", NO_HOME, REFUSED);
    };
    let mut plans = Vec::new();
    for (_, setup) in wiring.registry.lead_setups() {
        if setup.default_lead() {
            match setup.plan_uninstall(&home, args.force) {
                Ok(plan) => plans.push((setup, plan)),
                Err(e) => return fail("uninstall", &e.to_string(), REFUSED),
            }
        // Another harness's setup can be removed only through its program,
        // so a machine without it has none to remove.
        } else if version_of(wiring, setup.program()).is_some() {
            match setup.plan_uninstall(&home, args.force) {
                Ok(plan) => plans.push((setup, plan)),
                // An old or broken program must not keep the default setup in place.
                Err(e) => skipped("uninstall", setup.label(), &e.to_string()),
            }
        }
    }
    if plans.iter().all(|(_, p)| p.changes.is_empty()) {
        say_line("no lead setup to remove");
        return ExitCode::SUCCESS;
    }
    finish("uninstall", &plans, args.dry_run)
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
fn finish(command: &str, plans: &[Planned<'_>], dry_run: bool) -> ExitCode {
    if dry_run {
        for line in plans.iter().flat_map(|(_, p)| lines(p)) {
            say_line(&format!("would {line}"));
        }
        return ExitCode::SUCCESS;
    }
    let mut done = Vec::new();
    for (setup, plan) in plans {
        if let Err(e) = setup.apply(&plan.changes) {
            done.extend(e.done.iter().map(ToString::to_string));
            // Exit 2 says some change was made. A failure with nothing changed
            // is a plain refusal, so a script does not read it as a half-done
            // setup.
            let code = if done.is_empty() { REFUSED } else { PARTIAL };
            return fail(command, &partial_text(command, &e.to_string(), &done), code);
        }
        for line in lines(plan) {
            say_line(&format!("did {line}"));
            done.push(line);
        }
    }
    for note in plans.iter().filter_map(|(_, p)| p.note) {
        say_line(note);
    }
    ExitCode::SUCCESS
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
            lead_command(bin, Path::new("/u/.teamwire"), user).words(),
            ["/u/.cargo/bin/teamwire"]
        );
        assert_eq!(
            lead_command(bin, Path::new("/scratch/tw"), user).words(),
            ["/u/.cargo/bin/teamwire", "--home", "/scratch/tw"]
        );
        assert_eq!(
            lead_command(bin, Path::new("/u/.teamwire"), None).words(),
            ["/u/.cargo/bin/teamwire", "--home", "/u/.teamwire"]
        );
    }

    #[test]
    fn a_sentence_joins_its_words_with_the_last_one_apart() {
        let w = |ws: &[&str]| ws.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert_eq!(sentence(&w(&["a", "b", "c"]), "or"), "a, b or c");
        assert_eq!(sentence(&w(&["a", "b"]), "and"), "a and b");
        assert_eq!(sentence(&w(&["a"]), "or"), "a");
    }

    #[test]
    fn a_binary_under_target_is_a_build() {
        assert!(is_build_output(Path::new("/r/target/release/teamwire")));
        assert!(!is_build_output(Path::new("/u/.cargo/bin/teamwire")));
        assert!(!is_build_output(Path::new("/u/targets/teamwire")));
    }

    #[test]
    fn a_selector_line_keeps_all_or_picks_numbers() {
        const CLAUDE: &str = "claude-code";
        const CODEX: &str = "codex";
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
