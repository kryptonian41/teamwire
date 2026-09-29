//! `teamwire lead --adopt`: record this session as the team's lead.

use std::process::ExitCode;

use teamwire_core::TeamName;
use teamwire_msg::adopt_lead;

use crate::cmd::send::{caller_env, error_code, fail, say, with_ctx};
use crate::wiring::Wiring;
use crate::{Global, LeadArgs, USAGE};

pub(crate) fn run(args: &LeadArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let LeadArgs { adopt } = args;
    let team = match TeamName::parse(adopt) {
        Ok(team) => team,
        Err(e) => return fail("lead", &e.to_string(), USAGE),
    };
    let outcome = with_ctx(global, wiring, &team, |ctx| {
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
