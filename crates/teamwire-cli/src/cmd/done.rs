//! `teamwire done`: record a member's completion.

use std::process::ExitCode;

use teamwire_core::{Role, TeamName};
use teamwire_msg::CallerEnv;
use teamwire_report::{DoneError, DoneReport};
use teamwire_store::Ctx;

use crate::cmd::shared::{Opening, REFUSED, STORE, caller_env, fail, say, with_ctx_using};
use crate::wiring::Wiring;
use crate::{DoneArgs, Global};

pub(crate) fn run(args: &DoneArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let DoneArgs { status, summary } = args;
    let caller = caller_env(None);
    let (Some(team), Some(role), Some(_)) = (&caller.team, &caller.role, &caller.token) else {
        return fail(
            "done",
            "refused.identity: only a team member can record a completion \
             (TEAMWIRE_TEAM, TEAMWIRE_ROLE and TEAMWIRE_TOKEN are not all set)",
            REFUSED,
        );
    };
    let (Ok(team), Ok(role)) = (TeamName::parse(team), Role::parse(role)) else {
        return fail(
            "done",
            "refused.identity: the team or role name is not valid",
            REFUSED,
        );
    };
    let opening = Opening {
        command: "done",
        refusal_prefix: "refused.identity: ",
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "done", opening, |ctx| {
        record(ctx, &caller, &role, status, summary)
    })
    .unwrap_or_else(|code| code)
}

fn record(
    ctx: &mut Ctx<'_>,
    caller: &CallerEnv,
    role: &Role,
    status: &str,
    summary: &str,
) -> ExitCode {
    // The proof the message tools use, so a wrong token is logged as
    // `message.refused` here as it is for MCP `team_done`.
    if teamwire_msg::prove_actor_logged(ctx, caller).is_err() {
        return fail(
            "done",
            "refused.identity: the token does not belong to this member",
            REFUSED,
        );
    }
    match teamwire_report::done_with_status(ctx, role, status, summary) {
        Ok(DoneReport::Stored {
            completion,
            n,
            generation,
            status,
            report,
        }) => {
            say(&format!(
                "recorded completion {n} ({status}) for generation {generation} [{completion}]\n{report}\n"
            ));
            ExitCode::SUCCESS
        }
        Ok(DoneReport::NotStored { gaps, report, .. }) => {
            say(&format!(
                "nothing was stored yet.\n{report}\ngaps: {}\n",
                serde_json::to_string(&gaps).unwrap_or_default()
            ));
            ExitCode::from(REFUSED)
        }
        Err(e) => fail("done", &e.to_string(), error_code(&e)),
    }
}

fn error_code(e: &DoneError) -> u8 {
    match e {
        DoneError::Store(_) => STORE,
        DoneError::Invalid { .. }
        | DoneError::TeamClosed
        | DoneError::UnknownRole(_)
        | DoneError::Internal(_) => REFUSED,
    }
}
