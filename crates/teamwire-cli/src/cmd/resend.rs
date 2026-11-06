//! `teamwire resend`: retry one stored message.

use std::process::ExitCode;

use teamwire_core::{DisplayId, Recipient, TeamName};
use teamwire_msg::{CallerEnv, SendError, prove_actor_logged, resend};

use crate::cmd::send::{error_code, exit_of, print_report};
use crate::cmd::shared::{caller_env, fail, member_token, with_ctx};
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
        // claims to be the user when it holds a user key (ADR 0040), else the
        // lead, and the key or the harness session decides.
        let env = caller_env(None);
        let from = match (member_token(), &env.user_key) {
            (Some(_), _) => None,
            (None, Some(_)) => Some("user".to_owned()),
            (None, None) => Some("lead".to_owned()),
        };
        let actor = match prove_actor_logged(ctx, &CallerEnv { from, ..env }) {
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
