//! `teamwire received`: the lead read results, or messages to the lead,
//! that were not typed to it (ADR 0024, ADR 0043).

use std::process::ExitCode;

use teamwire_core::{Role, TeamName};
use teamwire_report::{Received, mark_received};

use crate::cmd::shared::{REFUSED, STORE, fail, refuse_member, say, with_ctx};
use crate::wiring::Wiring;
use crate::{Global, ReceivedArgs, USAGE};

pub(crate) fn run(args: &ReceivedArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ReceivedArgs { team, roles } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("received", &e.to_string(), USAGE),
    };
    let roles = match roles
        .iter()
        .map(|r| Role::parse(r).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(roles) => roles,
        Err(e) => return fail("received", &e, USAGE),
    };
    if let Some(code) = refuse_member("received") {
        return code;
    }
    match with_ctx(global, wiring, &team, "received", |ctx| {
        mark_received(ctx, &roles)
    }) {
        Err(code) => code,
        Ok(Err(e)) => fail("received", &e.to_string(), STORE),
        Ok(Ok(results)) => {
            let mut out = String::new();
            let mut unknown = false;
            for (role, outcome) in &results {
                let line = match outcome {
                    Received::Marked { n } => format!("{role} #{n} received\n"),
                    Received::NotWaiting { state: Some(state) } => {
                        format!("{role} unchanged: its newest result is {state}\n")
                    }
                    Received::NotWaiting { state: None } => {
                        format!("{role} unchanged: it has no result\n")
                    }
                    Received::NoMember => {
                        unknown = true;
                        format!("{role} refused: no member has this role\n")
                    }
                    Received::Read => format!("{role} read\n"),
                    Received::AlreadyDelivered => format!("{role} already delivered\n"),
                    Received::AlreadyRead => format!("{role} already read\n"),
                    Received::NoMessage => {
                        unknown = true;
                        format!("{role}: no such message\n")
                    }
                    Received::NotToLead => {
                        unknown = true;
                        format!("{role}: not a message to the lead\n")
                    }
                    Received::RoleAndMessage => {
                        unknown = true;
                        format!("{role}: both a role and a message; nothing recorded\n")
                    }
                };
                out.push_str(&line);
            }
            say(&out);
            if unknown {
                ExitCode::from(REFUSED)
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}
