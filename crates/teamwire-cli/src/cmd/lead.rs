//! `teamwire lead`: `--adopt` records this session as the team's lead;
//! `--handoff` hands every open team this session leads to a fresh lead
//! session in a pane beside this one (ADR 0051); `--handoff-guide` prints
//! what the old lead writes first; `--missed` lists what the old lead may
//! have been handed while it handed off.
//!
//! The order of the handoff's steps lives in `teamwire_launch::lead_hand_off`.
//! This file parses, calls it, prints what it says and maps its error to an
//! exit code.

use std::process::ExitCode;

use teamwire_core::{Config, HandoffFiles, TeamName};
use teamwire_launch::{
    HandoffError, LeadCaller, LeadHandoffCtx, LeadHandoffOpts, LeadHandoffWaits, LeadSeat, Said,
    find_lead, lead_hand_off,
};
use teamwire_msg::adopt_lead;
use teamwire_report::lead_missed;

use crate::cmd::send::error_code;
use crate::cmd::shared::{
    PARTIAL, REFUSED, STORE, caller_env, carries_member_identity, fail, home_or_refuse, say,
    say_line, warn, with_ctx,
};
use crate::wiring::Wiring;
use crate::{Global, LeadArgs, USAGE};

/// teamwire's guide for a lead's handoff document, adapted from the same
/// mattpocock-skills `handoff` skill as the member guide. `{{DOCUMENT}}` and
/// `{{TEAMS}}` are filled in for the caller.
const GUIDE: &str = include_str!("lead_handoff_guide.md");

pub(crate) fn run(args: &LeadArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    if let Some(team) = &args.adopt {
        return adopt(team, global, wiring);
    }
    if args.handoff {
        return handoff(args.dry_run, global, wiring);
    }
    let home = match home_or_refuse("lead", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let config = Config::default();
    let ctx = handoff_ctx(&home, wiring, &config);
    let env = caller_lookup;
    let seat = match find_lead(&ctx, &caller(&env)) {
        Ok(seat) => seat,
        Err(e) => return exit(&e),
    };
    if args.missed {
        missed(&home, &seat)
    } else {
        guide(&home, &seat)
    }
}

fn caller_lookup(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn caller(env: &dyn Fn(&str) -> Option<String>) -> LeadCaller<'_> {
    LeadCaller {
        env,
        is_member: carries_member_identity(),
    }
}

fn handoff_ctx<'a>(
    home: &'a std::path::Path,
    wiring: &'a Wiring,
    config: &'a Config,
) -> LeadHandoffCtx<'a> {
    LeadHandoffCtx {
        home,
        registry: &wiring.registry,
        platform: &*wiring.platform,
        clock: &wiring.clock,
        config,
    }
}

fn exit(e: &HandoffError) -> ExitCode {
    match e {
        HandoffError::Store(_) => fail("lead", &e.to_string(), STORE),
        HandoffError::Refused(why) => fail("lead", why, REFUSED),
        HandoffError::Partial(why) => fail("lead", why, PARTIAL),
    }
}

fn guide(home: &std::path::Path, seat: &LeadSeat) -> ExitCode {
    let Some(files) = HandoffFiles::of(home, &seat.session) else {
        return fail(
            "lead",
            &format!(
                "the session id {} is not a safe folder name, so it has no handoff folder",
                seat.session
            ),
            REFUSED,
        );
    };
    let teams: Vec<String> = seat.names().iter().map(ToString::to_string).collect();
    say(&GUIDE
        .replace("{{DOCUMENT}}", &files.document.display().to_string())
        .replace("{{TEAMS}}", &teams.join(", ")));
    ExitCode::SUCCESS
}

fn missed(home: &std::path::Path, seat: &LeadSeat) -> ExitCode {
    match lead_missed(home, &seat.session, &seat.names()) {
        Ok(lines) => {
            for line in lines {
                say_line(&line);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail("lead", &format!("the store could not be read: {e}"), STORE),
    }
}

fn adopt(team: &str, global: &Global, wiring: &Wiring) -> ExitCode {
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("lead", &e.to_string(), USAGE),
    };
    let outcome = with_ctx(global, wiring, &team, "lead", |ctx| {
        adopt_lead(ctx, &caller_env(Some("lead".to_owned())))
    });
    match outcome {
        Err(code) => code,
        Ok(Err(e)) => fail("lead", &e.to_string(), error_code(&e)),
        Ok(Ok(report)) => {
            let old = report.old.map_or("none".to_owned(), |s| s.to_string());
            say(&format!(
                "team {team}: lead session changed from {old} to {}\n",
                report.new
            ));
            ExitCode::SUCCESS
        }
    }
}

fn handoff(dry_run: bool, global: &Global, wiring: &Wiring) -> ExitCode {
    let home = match home_or_refuse("lead", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let config = Config::default();
    let ctx = handoff_ctx(&home, wiring, &config);
    let env = caller_lookup;
    // The new lead's shell does not have this command's flags, so a state
    // directory the caller named is passed to it.
    let named = global.home.is_some() || env("TEAMWIRE_HOME").is_some();
    let opts = LeadHandoffOpts {
        home_env: named.then(|| home.clone()),
        harness_env: wiring.harness_env.clone(),
        dry_run,
        waits: LeadHandoffWaits::of(&config),
    };
    let result = lead_hand_off(&ctx, &caller(&env), &opts, &mut |said| match said {
        Said::Line(text) => say_line(&text),
        Said::Warning(text) => warn(&text),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => exit(&e),
    }
}
