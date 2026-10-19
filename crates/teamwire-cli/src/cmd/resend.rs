//! `teamwire resend`: retry one stored message.

use std::process::ExitCode;

use teamwire_core::{DisplayId, Recipient, TeamName};
use teamwire_msg::{SendError, prove_actor_logged, resend};

use crate::cmd::send::{
    caller_env, error_code, exit_of, fail, member_token, print_report, with_ctx,
};
use crate::wiring::Wiring;
use crate::{Global, ResendArgs, USAGE};

pub(crate) fn run(args: &ResendArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ResendArgs { team, id, to } = args;
    let parsed = TeamName::parse(team)
        .map_err(|e| e.to_string())
        .and_then(|team| {
            let id = DisplayId::parse(id).map_err(|e| e.to_string())?;
            let to = to
                .as_ref()
                .map(|roles| {
                    roles
                        .iter()
                        .map(|r| Recipient::parse(r).map_err(|e| e.to_string()))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            Ok((team, id, to))
        });
    let (team, id, to) = match parsed {
        Ok(parts) => parts,
        Err(e) => return fail("resend", &e, USAGE),
    };
    let outcome = with_ctx(global, wiring, &team, "resend", |ctx| {
        // A member proves itself with its token. With no token the caller
        // claims to be the lead, and the harness session decides.
        let from = member_token().is_none().then(|| "lead".to_owned());
        let actor = match prove_actor_logged(ctx, &caller_env(from)) {
            Ok(actor) => actor,
            Err(refusal) => return Err(SendError::Refused(refusal)),
        };
        resend(ctx, &actor, &id, to.as_deref())
    });
    match outcome {
        Err(code) => code,
        Ok(Err(e)) => fail("resend", &e.to_string(), error_code(&e)),
        Ok(Ok(report)) => {
            print_report(&team, &report);
            ExitCode::from(exit_of(&report))
        }
    }
}
