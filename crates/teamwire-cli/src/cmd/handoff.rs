//! `teamwire handoff`: give one member a fresh session in the same seat, and
//! `--guide`, the guide a member follows before one.
//!
//! The order of the steps lives here because `teamwire-close` and
//! `teamwire-launch` may not depend on each other: launch's checks, close's
//! checks, close's place close (the host refuses a second live agent of one
//! name), then launch's open, trust, record and first line.

use std::process::ExitCode;

use teamwire_close::{close_handoff_place, handoff_refusals};
use teamwire_core::{Address, Role, TeamName};
use teamwire_launch::{HandoffError, open_handoff, plan_handoff};
use teamwire_store::Ctx;

use crate::cmd::send::{
    PARTIAL, REFUSED, STORE, fail, refuse_member, say, say_line, warn, with_ctx, with_read_ctx,
};
use crate::wiring::Wiring;
use crate::{Global, HandoffArgs, USAGE};

/// teamwire's copy of the mattpocock-skills `handoff` skill, adapted for a
/// member. It lives in the binary so it is the same for every harness.
const GUIDE: &str = include_str!("handoff_guide.md");

pub(crate) fn run(args: &HandoffArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    if args.guide {
        say(GUIDE);
        return ExitCode::SUCCESS;
    }
    let (Some(team), Some(role)) = (&args.team, &args.role) else {
        return fail("handoff", "name a team and --role, or pass --guide", USAGE);
    };
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("handoff", &e.to_string(), USAGE),
    };
    let role = match Role::parse(role) {
        Ok(role) => role,
        Err(e) => return fail("handoff", &e.to_string(), USAGE),
    };
    if let Some(code) = refuse_member("handoff") {
        return code;
    }
    let dry_run = args.dry_run;
    let work = |ctx: &mut Ctx<'_>| hand_off(ctx, &role, dry_run);
    // A dry run writes nothing, not even the migration of an older database.
    let outcome = if dry_run {
        with_read_ctx(global, wiring, &team, work)
    } else {
        with_ctx(global, wiring, &team, "handoff", work)
    };
    match outcome {
        Err(code) => code,
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err((why, code))) => fail("handoff", &why, code),
    }
}

/// Runs the steps in order and prints one line per step. An error carries
/// its exit code: `REFUSED` when nothing was touched, `STORE` when the store
/// cannot be read, `PARTIAL` after the old place was closed.
fn hand_off(ctx: &mut Ctx<'_>, role: &Role, dry_run: bool) -> Result<(), (String, u8)> {
    let plan = plan_handoff(ctx, role, std::env::current_exe().ok()).map_err(|e| match e {
        HandoffError::Refused(why) => (why, REFUSED),
        HandoffError::Store(_) => (e.to_string(), STORE),
    })?;
    let refusals = handoff_refusals(ctx, &plan.member);
    if !refusals.is_empty() {
        return Err((refusals.join("; "), REFUSED));
    }
    say_line(&format!(
        "{role}: started, idle and committed, so it can hand off"
    ));
    let old = plan.member.place.as_ref().map_or_else(
        || "no recorded place".to_owned(),
        |p| format!("place {}", p.id),
    );
    let session = plan.new_session.as_ref().map_or_else(
        || "the session it picks".to_owned(),
        |s| format!("session {s}"),
    );
    for w in &plan.warnings {
        warn(&format!("warning: {w}"));
    }
    if dry_run {
        say_line(&format!("{role}: would close {old}"));
        say_line(&format!(
            "{role}: would open {} in {} with {session}: {}",
            Address::of(&plan.team, role),
            plan.member.cwd.display(),
            plan.argv.join(" ")
        ));
        say_line(&format!(
            "{role}: would answer the trust dialog, record the handoff and send: {}",
            plan.first_line
        ));
        say_line("dry run: nothing was closed, opened or written");
        return Ok(());
    }
    match close_handoff_place(ctx, &plan.team, &plan.member) {
        Ok(Some(said)) => say_line(&said),
        Ok(None) => say_line(&format!("{role}: no recorded place, nothing to close")),
        Err(e) => {
            return Err((
                format!(
                    "{e}. No new session was opened. The old session is still recorded, so once \
                     it has ended, `teamwire reopen {} --role {role}` resumes it",
                    plan.team
                ),
                PARTIAL,
            ));
        }
    }
    let done = open_handoff(ctx, &plan).map_err(|e| (e, PARTIAL))?;
    say_line(&format!(
        "{role}: opened {session} in place {}, trust step done",
        done.place.id
    ));
    say_line(&format!(
        "{role}: handoff recorded, generation {}",
        done.generation
    ));
    for w in &done.warnings {
        warn(&format!("warning: {w}"));
    }
    if done.warnings.is_empty() {
        say_line(&format!(
            "{role}: first line sent, pointing at the brief and the handoff document"
        ));
    }
    Ok(())
}
