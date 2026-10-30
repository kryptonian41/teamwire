//! `teamwire done`: record a member's completion.

use std::process::ExitCode;

use teamwire_core::{Config, Role, TeamName};
use teamwire_report::{DoneError, DoneReport};
use teamwire_store::{Ctx, Store, StoreError};

use crate::cmd::shared::{REFUSED, STORE, caller_env, fail, home_or_refuse, no_database_text, say};
use crate::recorded::Recorded;
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
    let home = match home_or_refuse("done", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let mut store = match Store::open_existing(&home, &team) {
        Ok(store) => Recorded::new(store, &wiring.clock, "done"),
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(&team, &path);
            return fail("done", &format!("refused.identity: {message}"), REFUSED);
        }
        Err(e) => return fail("done", &e.to_string(), STORE),
    };
    let config = Config::default();
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    // The proof the message tools use, so a wrong token is logged as
    // `message.refused` here as it is for MCP `team_done`.
    if teamwire_msg::prove_actor_logged(&mut ctx, &caller).is_err() {
        return fail(
            "done",
            "refused.identity: the token does not belong to this member",
            REFUSED,
        );
    }
    match teamwire_report::done_with_status(&mut ctx, &role, status, summary) {
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
