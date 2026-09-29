//! `teamwire ack`: mark messages read without printing them.

use std::process::ExitCode;

use teamwire_core::{Actor, DisplayId, TeamName};
use teamwire_msg::{AckOutcome, SendError, ack, prove_actor};

use crate::cmd::send::{caller_env, error_code, fail, say, with_ctx};
use crate::wiring::Wiring;
use crate::{AckArgs, Global, USAGE};

pub(crate) fn run(args: &AckArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let AckArgs { ids } = args;
    let ids = match ids
        .iter()
        .map(|id| DisplayId::parse(id).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ids) => ids,
        Err(e) => return fail("ack", &e, USAGE),
    };
    let Some(team) = std::env::var("TEAMWIRE_TEAM")
        .ok()
        .and_then(|t| TeamName::parse(&t).ok())
    else {
        return fail("ack", "TEAMWIRE_TEAM does not name a team", USAGE);
    };
    let outcome = with_ctx(global, wiring, &team, |ctx| {
        match prove_actor(ctx, &caller_env(None)).map_err(SendError::Refused)? {
            Actor::Member(role) => ack(ctx, &role, &ids),
            Actor::Lead => Err(SendError::InvalidArgument(
                "the lead has no inbox to acknowledge".to_owned(),
            )),
        }
    });
    match outcome {
        Err(code) => code,
        Ok(Err(e)) => fail("ack", &e.to_string(), error_code(&e)),
        Ok(Ok(results)) => {
            for (label, wanted) in [
                ("acked", AckOutcome::Acked),
                ("already", AckOutcome::Already),
                ("unknown", AckOutcome::Unknown),
            ] {
                let named: Vec<&str> = results
                    .iter()
                    .filter(|r| r.outcome == wanted)
                    .map(|r| r.id.as_str())
                    .collect();
                if !named.is_empty() {
                    say(&format!("{label}: {}\n", named.join(" ")));
                }
            }
            ExitCode::SUCCESS
        }
    }
}
