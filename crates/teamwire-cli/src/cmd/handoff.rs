//! `teamwire handoff`: give one member a fresh session in the same seat, and
//! `--guide`, the guide a member follows before one.
//!
//! The order of the steps lives in `teamwire_launch::hand_off`. Launch and
//! close may not depend on each other, so this file hands it close's checks
//! and place close, then prints what it says and maps its error to an exit
//! code.

use std::process::ExitCode;

use teamwire_close::{close_handoff_place, handoff_refusals};
use teamwire_core::{Role, TeamName};
use teamwire_launch::{HandoffError, PlaceCloser, Said, hand_off};
use teamwire_store::Ctx;

use crate::cmd::shared::{
    PARTIAL, REFUSED, STORE, fail, refuse_member, say, say_line, warn, with_ctx, with_read_ctx,
};
use crate::wiring::Wiring;
use crate::{Global, HandoffArgs, USAGE};

/// teamwire's copy of the mattpocock-skills `handoff` skill, adapted for a
/// member. It lives in the binary so it is the same for every harness.
const GUIDE: &str = include_str!("handoff_guide.md");

const CLOSER: PlaceCloser = PlaceCloser {
    refusals: handoff_refusals,
    close: close_handoff_place,
};

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
    let work = |ctx: &mut Ctx<'_>| {
        hand_off(ctx, &role, CLOSER, dry_run, &mut |said| match said {
            Said::Line(text) => say_line(&text),
            Said::Warning(text) => warn(&text),
        })
    };
    // A dry run writes nothing, not even the migration of an older database.
    let outcome = if dry_run {
        with_read_ctx(global, wiring, &team, work)
    } else {
        with_ctx(global, wiring, &team, "handoff", work)
    };
    match outcome {
        Err(code) => code,
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e @ HandoffError::Store(_))) => fail("handoff", &e.to_string(), STORE),
        Ok(Err(HandoffError::Refused(why))) => fail("handoff", &why, REFUSED),
        Ok(Err(HandoffError::Partial(why))) => fail("handoff", &why, PARTIAL),
    }
}
