//! `teamwire reopen`: resume the sessions of a team whose sessions are gone.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_core::{Role, TeamName};
use teamwire_launch::{ReopenError, ReopenOpts, ReopenOutcome, ReopenReport, reopen};
use teamwire_store::Ctx;
use teamwire_watch::UndeliveredResult;

use crate::cmd::send::{fail, refuse_member, say, warn, with_ctx, with_read_ctx};
use crate::cmd::status::results_block;
use crate::wiring::Wiring;
use crate::{Global, ReopenArgs, USAGE};

/// Exit codes of `docs/api/cli.md`.
const REFUSED: u8 = 1;
const PARTIAL: u8 = 2;
const STORE_ERROR: u8 = 6;

pub(crate) fn run(args: &ReopenArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ReopenArgs {
        team,
        role,
        dry_run,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("reopen", &e.to_string(), USAGE),
    };
    let role = match role.as_deref().map(Role::parse).transpose() {
        Ok(role) => role,
        Err(e) => return fail("reopen", &e.to_string(), USAGE),
    };
    if let Some(code) = refuse_member("reopen") {
        return code;
    }
    let opts = ReopenOpts {
        role,
        dry_run: *dry_run,
        user_home: std::env::var_os("HOME").map(PathBuf::from),
        teamwire_bin: std::env::current_exe().ok(),
    };
    let work = |ctx: &mut Ctx<'_>| {
        let result = reopen(ctx, &opts);
        let unreceived = teamwire_watch::status_with(ctx, None).undelivered_results;
        (result, caller_is_lead(ctx), unreceived)
    };
    // A dry run writes nothing, not even the migration of an older database.
    let outcome = if opts.dry_run {
        with_read_ctx(global, wiring, &team, work)
    } else {
        with_ctx(global, wiring, &team, work)
    };
    match outcome {
        Err(code) => code,
        Ok((Err(e), _, _)) => {
            let code = match e {
                ReopenError::Store(_) => STORE_ERROR,
                ReopenError::NoTeam | ReopenError::NotOpen { .. } | ReopenError::NoMember(_) => {
                    REFUSED
                }
            };
            fail("reopen", &e.to_string(), code)
        }
        Ok((Ok(report), caller_is_lead, unreceived)) => {
            print_report(&team, &report, caller_is_lead, &unreceived);
            if report.has_problem() {
                ExitCode::from(PARTIAL)
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

/// True when the calling session is the one the team records as its lead.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn caller_is_lead(ctx: &Ctx<'_>) -> bool {
    let Ok(Some(team)) = ctx.store.read(|tx| tx.team()) else {
        return false;
    };
    let Ok(harness) = ctx.registry.harness(&team.lead_harness) else {
        return false;
    };
    match (harness.current_session(), team.lead_session) {
        (Some(caller), Some(lead)) => caller.id == lead,
        _ => false,
    }
}

fn line(who: &str, outcome: &ReopenOutcome, dry_run: bool) -> String {
    match outcome {
        ReopenOutcome::Reopened if dry_run => format!("{who} would reopen\n"),
        ReopenOutcome::Reopened => format!("{who} reopened\n"),
        ReopenOutcome::Skipped(why) => format!("{who} skipped: {why}\n"),
        ReopenOutcome::Refused(why) => format!("{who} refused: {why}\n"),
        ReopenOutcome::Failed(why) => format!("{who} failed: {why}\n"),
    }
}

fn print_report(
    team: &TeamName,
    report: &ReopenReport,
    caller_is_lead: bool,
    unreceived: &[UndeliveredResult],
) {
    let mut out = String::new();
    if let Some(lead) = &report.lead {
        out.push_str(&line("lead", lead, report.dry_run));
    }
    for (role, outcome) in &report.members {
        out.push_str(&line(role.as_str(), outcome, report.dry_run));
    }
    if report.dry_run {
        out.push_str("dry run: nothing was opened and nothing was written\n");
    }
    say(&out);
    for w in &report.warnings {
        warn(&format!("warning: {w}"));
    }
    let results = results_block(unreceived);
    if !results.is_empty() {
        let mut block = results.join("\n");
        block.push_str(
            "\nthe lead should read those files: teamwire never sends a result again by itself\n",
        );
        writeln!(
            block,
            "after reading them, run: teamwire received {team} <role>..."
        )
        .ok();
        say(&block);
    }
    let mut tail = format!("watch: {}\n", report.watch_command);
    tail.push_str(
        "a member that was killed mid-turn sits idle until the lead sends it a reprompt or a note\n",
    );
    if !caller_is_lead {
        writeln!(
            tail,
            "results go to the reopened lead; to make this session the lead instead, run: \
             teamwire lead --adopt {team}"
        )
        .ok();
    }
    say(&tail);
}
