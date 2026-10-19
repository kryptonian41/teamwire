//! `teamwire close`: close the team or remove one member.

use std::process::ExitCode;

use teamwire_close::{CloseOpts, ClosePlan, CloseReport, CloseStep, CloseTarget, StepOutcome};
use teamwire_core::{Config, Role, TeamName};
use teamwire_store::{Ctx, Store, StoreError};

use crate::cmd::send::{home_or_refuse, no_database_text, refuse_member};
use crate::recorded::Recorded;
use crate::wiring::Wiring;
use crate::{CloseArgs, Global, USAGE};

/// Refused by a rule; nothing changed (docs/api/cli.md).
const REFUSED: u8 = 1;
/// The close stopped at a step; a rerun continues from the first step.
const STOPPED: u8 = 2;
/// The store could not be opened or read.
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
        team: Some(team.clone()),
    };
    if !*dry_run && let Some(code) = refuse_member("close") {
        return code;
    }
    let home = match home_or_refuse("close", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    // A dry run opens the database read-only, so it cannot change anything.
    let opened = if *dry_run {
        Store::open_read_only(&home, &team)
    } else {
        Store::open_existing(&home, &team)
    };
    let mut store = match opened {
        Ok(store) => Recorded::new(store, &wiring.clock, "close"),
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(&team, &path);
            return fail(&message, REFUSED);
        }
        Err(e) => return fail(&e.to_string(), STORE),
    };
    // A row that does not parse is a broken store (exit 6), not a rule that
    // refused, so it is read here before the close library turns it into one.
    if let Err(e) = store.read(|tx| {
        tx.team()?;
        tx.members()?.all_or_corrupt()
    }) {
        return fail(&format!("the team cannot be read: {e}"), STORE);
    }
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

/// The steps of a whole-team close that take something away. A failed one may
/// have done part of its work (one worktree removed, the next refused).
const DESTRUCTIVE: [&str; 3] = ["remove_worktrees", "close_places", "release_hold"];

/// What state a stopped close leaves. Once the `finish` step is done the team
/// is `closed`, so "the state is intact" would be wrong: the team is closed and
/// only the archive or the deletion is left. Before that, the state is intact
/// only when no destructive step ran; otherwise the team is open again with
/// some worktrees, places or the hold already gone, and a rerun finishes them.
fn stopped_message(report: &CloseReport, step: &str, team: &TeamName) -> String {
    let finished = report.steps.iter().any(|s| s.step == "finish" && s.ok);
    let ran: Vec<&str> = DESTRUCTIVE
        .into_iter()
        .filter(|d| report.steps.iter().any(|s| s.step == *d))
        .collect();
    if finished {
        format!(
            "close stopped at {step}: the team is already marked closed and its state \
             directory is still there; fix that, then run `teamwire close {team}` again to \
             archive it afresh and delete it"
        )
    } else if ran.is_empty() {
        format!(
            "close stopped at {step}: the team is open again with its state intact; fix \
             that, then run the close again"
        )
    } else {
        format!(
            "close stopped at {step}: the team is open again, but {} already ran and close \
             does not undo that; fix the problem, then run `teamwire close {team}` again to \
             finish the job",
            ran.join(", ")
        )
    }
}

/// What a stopped removal of one member leaves. The team never left `open`,
/// so only the member's own steps count. Each acts on one member with one
/// call, so a failed one took nothing away; one that succeeded did.
fn removal_stopped_message(
    report: &CloseReport,
    step: &str,
    team: &TeamName,
    role: &Role,
) -> String {
    let ran: Vec<&str> = ["close_place", "remove_worktree"]
        .into_iter()
        .filter(|d| report.steps.iter().any(|s| s.step == *d && s.ok))
        .collect();
    let rerun = format!("`teamwire close {team} --role {role}`");
    if ran.is_empty() {
        format!(
            "removal of {role} stopped at {step}: nothing was taken away; fix that, then run \
             {rerun} again"
        )
    } else {
        format!(
            "removal of {role} stopped at {step}: {} already ran and close does not undo \
             that; {role}'s row is kept, not marked removed. Fix the problem, then run \
             {rerun} again to finish the job",
            ran.join(", ")
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
        warn(&match target {
            CloseTarget::Team => stopped_message(report, step, team),
            CloseTarget::Member(role) => removal_stopped_message(report, step, team, role),
        });
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
        let before = report(&[("lock", true), ("archive", true), ("verify_archive", false)]);
        let text = stopped_message(&before, "verify_archive", &team);
        assert!(text.contains("open again with its state intact"), "{text}");
        let failed_finish = report(&[("lock", true), ("finish", false)]);
        assert!(stopped_message(&failed_finish, "finish", &team).contains("state intact"));
    }

    #[test]
    fn a_stop_after_a_destructive_step_names_it_and_says_a_rerun_finishes() {
        let team = TeamName::parse("alpha").unwrap();
        let after = report(&[
            ("lock", true),
            ("archive", true),
            ("verify_archive", true),
            ("remove_worktrees", true),
            ("close_places", false),
        ]);
        let text = stopped_message(&after, "close_places", &team);
        assert_eq!(
            text,
            "close stopped at close_places: the team is open again, but remove_worktrees, \
             close_places already ran and close does not undo that; fix the problem, then \
             run `teamwire close alpha` again to finish the job"
        );
        let failed_finish = report(&[
            ("lock", true),
            ("remove_worktrees", true),
            ("close_places", true),
            ("release_hold", true),
            ("finish", false),
        ]);
        let text = stopped_message(&failed_finish, "finish", &team);
        assert!(!text.contains("state intact"), "{text}");
        assert!(
            text.contains("remove_worktrees, close_places, release_hold already ran"),
            "{text}"
        );
    }

    #[test]
    fn a_stopped_removal_names_what_it_already_did() {
        let team = TeamName::parse("alpha").unwrap();
        let role = Role::parse("api").unwrap();
        let after = report(&[("close_place", true), ("remove_worktree", false)]);
        let text = removal_stopped_message(&after, "remove_worktree", &team, &role);
        assert_eq!(
            text,
            "removal of api stopped at remove_worktree: close_place already ran and close \
             does not undo that; api's row is kept, not marked removed. Fix the problem, then \
             run `teamwire close alpha --role api` again to finish the job"
        );
        assert!(!text.contains("state intact"), "{text}");
        let before = report(&[("close_place", false)]);
        let text = removal_stopped_message(&before, "close_place", &team, &role);
        assert!(text.contains("nothing was taken away"), "{text}");
    }
}
