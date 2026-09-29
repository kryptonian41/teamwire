//! `teamwire close`: close the team or remove one member.

use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_close::{CloseOpts, ClosePlan, CloseReport, CloseStep, CloseTarget, StepOutcome};
use teamwire_core::{Config, Role, TeamName};
use teamwire_store::{Ctx, Store, StoreError};

use crate::wiring::Wiring;
use crate::{CloseArgs, Global, USAGE};

/// Refused by a rule; nothing changed (docs/api/cli.md).
const REFUSED: u8 = 1;
/// The close stopped at a step; state is intact and a rerun continues.
const STOPPED: u8 = 2;
/// The store could not be opened.
const STORE: u8 = 6;

pub(crate) fn run(args: &CloseArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let CloseArgs {
        team,
        role,
        stop,
        unverified,
        dry_run,
        keep_state,
        archive_to,
        clear_stale_lock,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    let target = match role.as_deref().map(Role::parse).transpose() {
        Ok(Some(role)) => CloseTarget::Member(role),
        Ok(None) => CloseTarget::Team,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    let opts = CloseOpts {
        stop: *stop,
        unverified: *unverified,
        keep_state: *keep_state,
        archive_to: archive_to.clone(),
        clear_stale_lock: *clear_stale_lock,
    };
    let home = state_home(global);
    // A dry run opens the database read-only, so it cannot change anything.
    let opened = if *dry_run {
        Store::open_read_only(&home, &team)
    } else {
        Store::open_existing(&home, &team)
    };
    let mut store = match opened {
        Ok(store) => store,
        Err(StoreError::Io(message)) if message.starts_with("no database") => {
            return fail(&message, REFUSED);
        }
        Err(e) => return fail(&e.to_string(), STORE),
    };
    let config = Config::default();
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    if *dry_run {
        let plan = teamwire_close::plan_close_with(&ctx, target, &opts);
        return show_plan(&plan);
    }
    let report = teamwire_close::close(&mut ctx, target.clone(), opts);
    show_report(&report, &target, &team)
}

/// `--home`, else `$TEAMWIRE_HOME`, else `~/.teamwire`.
fn state_home(global: &Global) -> PathBuf {
    let Global { home } = global;
    home.clone()
        .or_else(|| std::env::var_os("TEAMWIRE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".teamwire")))
        .unwrap_or_else(|| PathBuf::from(".teamwire"))
}

fn show_plan(plan: &ClosePlan) -> ExitCode {
    for CloseStep { step, detail } in &plan.steps {
        say(&format!("[dry-run] {step}: {detail}"));
    }
    if plan.refusals.is_empty() {
        return ExitCode::SUCCESS;
    }
    for why in &plan.refusals {
        warn(&format!("REFUSED: {why}"));
    }
    ExitCode::from(REFUSED)
}

fn show_report(report: &CloseReport, target: &CloseTarget, team: &TeamName) -> ExitCode {
    for StepOutcome { step, ok, detail } in &report.steps {
        if *ok {
            say(&format!("{step}: {detail}"));
        } else {
            warn(&format!("PROBLEM {step}: {detail}"));
        }
    }
    if !report.refusals.is_empty() {
        for why in &report.refusals {
            warn(&format!("REFUSED: {why}"));
        }
        warn("nothing was changed");
        return ExitCode::from(REFUSED);
    }
    for line in &report.unpushed {
        say(&format!("unpushed: {line}"));
    }
    if !report.branches_kept.is_empty() {
        say(&format!(
            "branches kept: {}",
            report.branches_kept.join(", ")
        ));
    }
    if let Some(step) = &report.stopped_at {
        if let Some(archive) = &report.archive {
            warn(&format!("archive so far: {}", archive.display()));
        }
        warn(&format!(
            "close stopped at {step}: the state is intact; fix that, then run the close again"
        ));
        return ExitCode::from(STOPPED);
    }
    match target {
        CloseTarget::Team => match &report.archive {
            Some(archive) => say(&format!(
                "closed team {team}. Archive: {}",
                archive.display()
            )),
            None => say(&format!("team {team}: done")),
        },
        CloseTarget::Member(role) => say(&format!("removed {role} from team {team}")),
    }
    ExitCode::SUCCESS
}

#[expect(clippy::print_stdout, reason = "the steps are the command's output")]
fn say(text: &str) {
    println!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal or a problem on stderr"
)]
fn warn(text: &str) {
    eprintln!("{text}");
}

fn fail(message: &str, code: u8) -> ExitCode {
    warn(&format!("teamwire close: {message}"));
    ExitCode::from(code)
}
