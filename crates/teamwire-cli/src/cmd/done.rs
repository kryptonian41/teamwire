//! `teamwire done`: record a member's completion.

use std::process::ExitCode;

use teamwire_core::{Config, Role, TeamName};
use teamwire_report::{DoneError, DoneReport};
use teamwire_store::{Ctx, Store, StoreError};

use crate::cmd::send::{home_or_refuse, no_database_text};
use crate::wiring::Wiring;
use crate::{DoneArgs, Global};

/// Refused by a rule; nothing changed (docs/api/cli.md).
const REFUSED: u8 = 1;
/// The store could not be read or written.
const STORE: u8 = 6;

/// Who is calling, from the member's environment (ADR 0008).
struct Caller {
    team: Option<String>,
    role: Option<String>,
    token: Option<String>,
}

impl Caller {
    fn from_process() -> Self {
        let var = |name| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self {
            team: var("TEAMWIRE_TEAM"),
            role: var("TEAMWIRE_ROLE"),
            token: teamwire_msg::caller_token(|name| std::env::var(name).ok()),
        }
    }
}

pub(crate) fn run(args: &DoneArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let DoneArgs { status, summary } = args;
    let caller = Caller::from_process();
    let (Some(team), Some(role), Some(token)) = (caller.team, caller.role, caller.token) else {
        return fail(
            "refused.identity: only a team member can record a completion \
             (TEAMWIRE_TEAM, TEAMWIRE_ROLE and TEAMWIRE_TOKEN are not all set)",
            REFUSED,
        );
    };
    let (Ok(team), Ok(role)) = (TeamName::parse(&team), Role::parse(&role)) else {
        return fail(
            "refused.identity: the team or role name is not valid",
            REFUSED,
        );
    };
    let home = match home_or_refuse("done", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let mut store = match Store::open_existing(&home, &team) {
        Ok(store) => store,
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(&team, &path);
            return fail(&format!("refused.identity: {message}"), REFUSED);
        }
        Err(e) => return fail(&e.to_string(), STORE),
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
    if !teamwire_report::token_matches(&ctx, &team, &role, &token) {
        return fail(
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
        Err(e) => fail(&e.to_string(), error_code(&e)),
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

#[expect(
    clippy::print_stdout,
    reason = "what happens next is the command's output"
)]
fn say(text: &str) {
    print!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal on stderr"
)]
fn fail(message: &str, code: u8) -> ExitCode {
    eprintln!("teamwire done: {message}");
    ExitCode::from(code)
}
