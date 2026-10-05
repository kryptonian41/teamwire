//! `teamwire close`: close the team or remove one member.

use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_close::{CloseOpts, ClosePlan, CloseReport, CloseStep, CloseTarget, StepOutcome};
use teamwire_core::{Config, Role, TeamName};
use teamwire_store::{Ctx, Store, StoreError};

use crate::cmd::send::refuse_member;
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
    if !*dry_run {
        if let Some(code) = refuse_member("close") {
            return code;
        }
    }
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

/// What state a stopped close leaves. Once the `finish` step is done the team
/// is `closed`, so "the state is intact" would be wrong: the team is closed and
/// only the archive or the deletion is left.
fn stopped_message(report: &CloseReport, step: &str, team: &TeamName) -> String {
    let finished = report.steps.iter().any(|s| s.step == "finish" && s.ok);
    if finished {
        format!(
            "close stopped at {step}: the team is already marked closed and its state \
             directory is still there; fix that, then run `teamwire close {team}` again to \
             archive it afresh and delete it"
        )
    } else {
        format!(
            "close stopped at {step}: the team is open again with its state intact; fix \
             that, then run the close again"
        )
    }
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
        warn(&stopped_message(report, step, team));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn report(steps: &[(&str, bool)]) -> CloseReport {
        CloseReport {
            steps: steps
                .iter()
                .map(|(step, ok)| StepOutcome {
                    step: (*step).to_owned(),
                    ok: *ok,
                    detail: String::new(),
                })
                .collect(),
            ..CloseReport::default()
        }
    }

    #[test]
    fn a_stop_after_finish_says_the_team_is_closed() {
        let team = TeamName::parse("alpha").unwrap();
        let after = report(&[("lock", true), ("finish", true), ("delete_state", false)]);
        let text = stopped_message(&after, "delete_state", &team);
        assert!(text.contains("already marked closed"), "{text}");
        assert!(!text.contains("state intact"), "{text}");
        let before = report(&[("lock", true), ("close_places", false)]);
        let text = stopped_message(&before, "close_places", &team);
        assert!(text.contains("open again with its state intact"), "{text}");
        let failed_finish = report(&[("lock", true), ("finish", false)]);
        assert!(stopped_message(&failed_finish, "finish", &team).contains("state intact"));
    }
}
