//! `teamwire lead`: `--adopt` records this session as the team's lead;
//! `--handoff` hands every open team this session leads to a fresh lead
//! session in a pane beside this one (ADR 0051).
//!
//! The order of the handoff's steps lives in `teamwire_launch::lead_hand_off`.
//! This file parses, calls it, prints what it says and maps its error to an
//! exit code.

use std::process::ExitCode;

use teamwire_core::{Config, TeamName};
use teamwire_launch::{
    HandoffError, LeadCaller, LeadHandoffCtx, LeadHandoffOpts, LeadHandoffWaits, Said,
    lead_hand_off,
};
use teamwire_msg::adopt_lead;

use crate::cmd::send::error_code;
use crate::cmd::shared::{
    PARTIAL, REFUSED, STORE, caller_env, carries_member_identity, fail, home_or_refuse, say,
    say_line, warn, with_ctx,
};
use crate::wiring::Wiring;
use crate::{Global, LeadArgs, USAGE};

pub(crate) fn run(args: &LeadArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    match &args.adopt {
        Some(team) => adopt(team, global, wiring),
        None => handoff(args.dry_run, global, wiring),
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
    let ctx = LeadHandoffCtx {
        home: &home,
        registry: &wiring.registry,
        platform: &*wiring.platform,
        clock: &wiring.clock,
        config: &config,
    };
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let caller = LeadCaller {
        env: &env,
        is_member: carries_member_identity(),
    };
    // The new lead's shell does not have this command's flags, so a state
    // directory the caller named is passed to it.
    let named = global.home.is_some() || env("TEAMWIRE_HOME").is_some();
    let opts = LeadHandoffOpts {
        home_env: named.then(|| home.clone()),
        dry_run,
        waits: LeadHandoffWaits::of(&config),
    };
    let result = lead_hand_off(&ctx, &caller, &opts, &mut |said| match said {
        Said::Line(text) => say_line(&text),
        Said::Warning(text) => warn(&text),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e @ HandoffError::Store(_)) => fail("lead", &e.to_string(), STORE),
        Err(HandoffError::Refused(why)) => fail("lead", &why, REFUSED),
        Err(HandoffError::Partial(why)) => fail("lead", &why, PARTIAL),
    }
}
