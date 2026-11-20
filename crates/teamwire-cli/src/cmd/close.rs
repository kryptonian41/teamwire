//! `teamwire close`: close the team or remove one member.

use std::process::ExitCode;

use teamwire_close::{
    CloseOpts, ClosePlan, CloseReport, CloseStep, CloseTarget, Expected, StepOutcome,
};
use teamwire_core::{Role, TeamName, TeamState};

use crate::cmd::shared::{
    Opening, PARTIAL, REFUSED, STORE, fail, refuse_member, say_line, warn, with_ctx_using,
};
use crate::wiring::Wiring;
use crate::{CloseArgs, Global, USAGE};

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
        expect,
        abandon,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("close", &e.to_string(), USAGE),
    };
    let target = match role.as_deref().map(Role::parse).transpose() {
        Ok(Some(role)) => CloseTarget::Member(role),
        Ok(None) => CloseTarget::Team,
        Err(e) => return fail("close", &e.to_string(), USAGE),
    };
    let expect = match expect
        .iter()
        .map(|e| Expected::parse(e))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(expect) => expect,
        Err(why) => return fail("close", &why, USAGE),
    };
    if abandon.as_deref().is_some_and(|why| why.trim().is_empty()) {
        return fail(
            "close",
            "--abandon needs the reason the team closes as it is",
            USAGE,
        );
    }
    let opts = CloseOpts {
        expect,
        abandon: abandon.clone(),
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
    // A dry run opens the database read-only, so it cannot change anything.
    let opening = Opening {
        command: "close",
        read_only: *dry_run,
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "close", opening, |ctx| {
        // A row that does not parse is a broken store (exit 6), not a rule
        // that refused, so it is read here before the close library turns it
        // into one.
        if let Err(e) = ctx.store.read(|tx| {
            tx.team()?;
            tx.members()?.all_or_corrupt()
        }) {
            return fail("close", &format!("the team cannot be read: {e}"), STORE);
        }
        if *dry_run {
            let plan = teamwire_close::plan_close_with(ctx, target, &opts);
            return show_plan(&plan);
        }
        let report = teamwire_close::close(ctx, target.clone(), opts);
        let lock = if report.stopped_at.is_some() && target == CloseTarget::Team {
            lock_after_stop(ctx)
        } else {
            Lock::GivenBack
        };
        show_report(&report, &target, &team, lock)
    })
    .unwrap_or_else(|code| code)
}

/// The steps of a whole-team close that take something away. A failed one may
/// have done part of its work (one worktree removed, the next refused).
const DESTRUCTIVE: [&str; 3] = ["remove_worktrees", "close_places", "release_hold"];

/// How `teamwire-close` starts the detail of a destructive step whose re-check
/// under the lock refused (CLS-3). That re-check runs before the step touches
/// anything, so such a step removed nothing.
const RECHECK_REFUSED: &str = "the world changed under the lock: ";

/// Whether `outcome` is a destructive step that refused at its re-check.
fn refused_at_recheck(outcome: &StepOutcome) -> bool {
    !outcome.ok && outcome.detail.starts_with(RECHECK_REFUSED)
}

/// Whether a whole-team close that stopped gave its lock back. The close
/// gives the lock back when a step fails, but that is one more store write and
/// it can fail too. The close library only logs that failure, so the CLI reads
/// the team again to tell. A team still `closing` refuses every new close until
/// `--clear-stale-lock` clears the lock of the dead process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lock {
    GivenBack,
    Kept,
    /// The team could not be read after the stop.
    Unknown,
}

fn lock_after_stop(ctx: &teamwire_store::Ctx<'_>) -> Lock {
    match ctx.store.read(|tx| Ok(tx.team()?.map(|team| team.state))) {
        Ok(Some(TeamState::Closing)) => Lock::Kept,
        Ok(Some(_)) => Lock::GivenBack,
        Ok(None) | Err(_) => Lock::Unknown,
    }
}

/// What state a stopped close leaves. Once the `finish` step is done the team
/// is `closed`, so "the state is intact" would be wrong: the team is closed and
/// only the archive or the deletion is left. Before that, the state is intact
/// only when no destructive step ran; otherwise the team is open again with
/// some worktrees, places or the hold already gone, and a rerun finishes them.
/// A step that refused at its re-check did not run. A lock that was not given
/// back comes before all of that: the team is not open, and only clearing the
/// lock lets the close run again.
fn stopped_message(report: &CloseReport, step: &str, team: &TeamName, lock: Lock) -> String {
    let finished = report.steps.iter().any(|s| s.step == "finish" && s.ok);
    let ran: Vec<&str> = DESTRUCTIVE
        .into_iter()
        .filter(|d| {
            report
                .steps
                .iter()
                .any(|s| s.step == *d && !refused_at_recheck(s))
        })
        .collect();
    let refused = report
        .steps
        .iter()
        .any(|s| s.step == step && refused_at_recheck(s));
    if finished {
        format!(
            "close stopped at {step}: the team is already marked closed and its state \
             directory is still there; fix that, then run `teamwire close {team}` again to \
             archive it afresh and delete it"
        )
    } else if lock != Lock::GivenBack {
        let why = if lock == Lock::Kept {
            "the close lock could not be given back, so the team is still closing and a new \
             close is refused"
        } else {
            "the team could not be read afterwards, so the close lock may still be held"
        };
        let undone = if ran.is_empty() {
            String::new()
        } else {
            format!(
                "; {} already ran and close does not undo that",
                ran.join(", ")
            )
        };
        format!(
            "close stopped at {step}, and {why}{undone}; fix that, then run `teamwire close \
             {team} --clear-stale-lock` and then `teamwire close {team}` again"
        )
    } else if refused && ran.is_empty() {
        format!(
            "close stopped at {step}: {step} refused at its re-check under the lock and \
             removed nothing, so the team is open again with its state intact; fix that, then \
             run the close again"
        )
    } else if refused {
        format!(
            "close stopped at {step}: {step} refused at its re-check under the lock and \
             removed nothing, but the team is open again with {} already run, and close does \
             not undo that; fix the problem, then run `teamwire close {team}` again to finish \
             the job",
            ran.join(", ")
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
/// call, so a failed one took nothing away; one that succeeded did. One
/// failure comes after the work: a `close_place` whose detail starts with
/// `<role>'s place ` closed the place, then found processes from it still
/// running, so the worktree was kept.
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
    let place = format!("{role}'s place ");
    let closed = report
        .steps
        .iter()
        .any(|s| s.step == "close_place" && !s.ok && s.detail.starts_with(&place));
    if closed {
        format!(
            "removal of {role} stopped at {step}: {role}'s place was closed and its worktree \
             kept, because processes from the place still run; {role}'s row is kept, not \
             marked removed. Stop them, then run {rerun} again to finish the job"
        )
    } else if ran.is_empty() {
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

/// A whole-team close while no closer has reported. It is said first, and
/// the close goes on.
fn show_closer_warning(warning: Option<&String>) {
    if let Some(text) = warning {
        warn(&format!("warning: {text}"));
    }
}

fn show_plan(plan: &ClosePlan) -> ExitCode {
    show_closer_warning(plan.closer_warning.as_ref());
    for CloseStep { step, detail } in &plan.steps {
        say_line(&format!("[dry-run] {step}: {detail}"));
    }
    if plan.refusals.is_empty() {
        return ExitCode::SUCCESS;
    }
    for why in &plan.refusals {
        warn(&format!("REFUSED: {why}"));
    }
    ExitCode::from(REFUSED)
}

fn show_report(
    report: &CloseReport,
    target: &CloseTarget,
    team: &TeamName,
    lock: Lock,
) -> ExitCode {
    show_closer_warning(report.closer_warning.as_ref());
    for StepOutcome { step, ok, detail } in &report.steps {
        if *ok {
            say_line(&format!("{step}: {detail}"));
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
        say_line(&format!("unpushed: {line}"));
    }
    if !report.branches_kept.is_empty() {
        say_line(&format!(
            "branches kept: {}",
            report.branches_kept.join(", ")
        ));
    }
    if let Some(step) = &report.stopped_at {
        if let Some(archive) = &report.archive {
            warn(&format!("archive so far: {}", archive.display()));
        }
        warn(&match target {
            CloseTarget::Team => stopped_message(report, step, team, lock),
            CloseTarget::Member(role) => removal_stopped_message(report, step, team, role),
        });
        return ExitCode::from(PARTIAL);
    }
    match target {
        CloseTarget::Team => match &report.archive {
            Some(archive) => say_line(&format!(
                "closed team {team}. Archive: {}",
                archive.display()
            )),
            None => say_line(&format!("team {team}: done")),
        },
        CloseTarget::Member(role) => say_line(&format!("removed {role} from team {team}")),
    }
    ExitCode::SUCCESS
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
        let text = stopped_message(&after, "delete_state", &team, Lock::GivenBack);
        assert!(text.contains("already marked closed"), "{text}");
        assert!(!text.contains("state intact"), "{text}");
        let before = report(&[("lock", true), ("archive", true), ("verify_archive", false)]);
        let text = stopped_message(&before, "verify_archive", &team, Lock::GivenBack);
        assert!(text.contains("open again with its state intact"), "{text}");
        let failed_finish = report(&[("lock", true), ("finish", false)]);
        assert!(
            stopped_message(&failed_finish, "finish", &team, Lock::GivenBack)
                .contains("state intact")
        );
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
        let text = stopped_message(&after, "close_places", &team, Lock::GivenBack);
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
        let text = stopped_message(&failed_finish, "finish", &team, Lock::GivenBack);
        assert!(!text.contains("state intact"), "{text}");
        assert!(
            text.contains("remove_worktrees, close_places, release_hold already ran"),
            "{text}"
        );
    }

    #[test]
    fn a_destructive_step_refused_at_its_recheck_is_not_named_as_ran() {
        let team = TeamName::parse("alpha").unwrap();
        // The text teamwire-close writes when the re-check under the lock
        // refuses a destructive step before it touches anything.
        let refused = |step: &str| StepOutcome {
            step: step.to_owned(),
            ok: false,
            detail: "the world changed under the lock: api is busy: its current turn is lost \
                     if its place closes. Wait for it, or remove it alone with --role api --stop"
                .to_owned(),
        };
        let mut first = report(&[("lock", true), ("archive", true), ("verify_archive", true)]);
        first.steps.push(refused("remove_worktrees"));
        let text = stopped_message(&first, "remove_worktrees", &team, Lock::GivenBack);
        assert_eq!(
            text,
            "close stopped at remove_worktrees: remove_worktrees refused at its re-check under \
             the lock and removed nothing, so the team is open again with its state intact; \
             fix that, then run the close again"
        );
        let mut second = report(&[
            ("lock", true),
            ("archive", true),
            ("verify_archive", true),
            ("remove_worktrees", true),
        ]);
        second.steps.push(refused("close_places"));
        let text = stopped_message(&second, "close_places", &team, Lock::GivenBack);
        assert_eq!(
            text,
            "close stopped at close_places: close_places refused at its re-check under the \
             lock and removed nothing, but the team is open again with remove_worktrees \
             already run, and close does not undo that; fix the problem, then run `teamwire \
             close alpha` again to finish the job"
        );
    }

    #[test]
    fn a_stop_that_kept_the_lock_says_the_team_is_closing_and_names_the_clear() {
        let team = TeamName::parse("alpha").unwrap();
        let before = report(&[("lock", true), ("archive", false)]);
        let text = stopped_message(&before, "archive", &team, Lock::Kept);
        assert_eq!(
            text,
            "close stopped at archive, and the close lock could not be given back, so the team \
             is still closing and a new close is refused; fix that, then run `teamwire close \
             alpha --clear-stale-lock` and then `teamwire close alpha` again"
        );
        let after = report(&[
            ("lock", true),
            ("remove_worktrees", true),
            ("close_places", false),
        ]);
        let text = stopped_message(&after, "close_places", &team, Lock::Kept);
        assert_eq!(
            text,
            "close stopped at close_places, and the close lock could not be given back, so the \
             team is still closing and a new close is refused; remove_worktrees, close_places \
             already ran and close does not undo that; fix that, then run `teamwire close \
             alpha --clear-stale-lock` and then `teamwire close alpha` again"
        );
        let text = stopped_message(&before, "archive", &team, Lock::Unknown);
        assert_eq!(
            text,
            "close stopped at archive, and the team could not be read afterwards, so the close \
             lock may still be held; fix that, then run `teamwire close alpha \
             --clear-stale-lock` and then `teamwire close alpha` again"
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

    #[test]
    fn a_removal_stopped_after_its_place_closed_says_the_worktree_is_kept() {
        let team = TeamName::parse("alpha").unwrap();
        let role = Role::parse("api").unwrap();
        // The text teamwire-close writes when processes outlive the closed place.
        let closed = CloseReport {
            steps: vec![StepOutcome {
                step: "close_place".to_owned(),
                ok: false,
                detail: "api's place p1 closed, but process(es) 7002 that ran in it still run \
                         5 s later; the worktree is kept. Stop them, then run close again"
                    .to_owned(),
            }],
            ..CloseReport::default()
        };
        let text = removal_stopped_message(&closed, "close_place", &team, &role);
        assert_eq!(
            text,
            "removal of api stopped at close_place: api's place was closed and its worktree \
             kept, because processes from the place still run; api's row is kept, not marked \
             removed. Stop them, then run `teamwire close alpha --role api` again to finish \
             the job"
        );
        assert!(!text.contains("nothing was taken away"), "{text}");
    }
}
