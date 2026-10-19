//! `teamwire teams`: every team on this machine with its state and
//! members, so an agent can find who to message in another team. Reads
//! only, and needs no proof: the roster holds no place, token or session id.

use std::process::ExitCode;

use serde_json::json;
use teamwire_msg::{TeamEntry, describe};

use crate::cmd::members::render;
use crate::cmd::send::{STORE, fail, home_or_refuse, say, warn, with_read_ctx};
use crate::wiring::Wiring;
use crate::{Global, TeamsArgs};

/// ` (refs GKA-580, #1)`, or nothing for a team without refs.
fn refs_suffix(refs: &[String]) -> String {
    if refs.is_empty() {
        String::new()
    } else {
        format!(" (refs {})", refs.join(", "))
    }
}

pub(crate) fn run(args: &TeamsArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let home = match home_or_refuse("teams", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let names = match teamwire_store::team_names(&home) {
        Ok(names) => names,
        Err(e) => return fail("teams", &e.to_string(), STORE),
    };
    let mut entries: Vec<TeamEntry> = Vec::new();
    let mut code = ExitCode::SUCCESS;
    for name in &names {
        match with_read_ctx(global, wiring, name, |ctx| describe(ctx)) {
            Ok(Ok(entry)) => entries.push(entry),
            Ok(Err(e)) => {
                warn(&format!("team {name} could not be read: {e}"));
                code = ExitCode::from(STORE);
            }
            Err(failed) => code = failed,
        }
    }
    if args.json {
        say(&format!("{}\n", json!({ "teams": entries })));
        return code;
    }
    if entries.is_empty() {
        say("no teams\n");
    }
    for entry in &entries {
        say(&format!(
            "{} ({}) {}: address as lead@{} or <role>@{}{}\n",
            entry.name,
            entry.state,
            entry.title,
            entry.name,
            entry.name,
            refs_suffix(&entry.refs)
        ));
        for member in &entry.members {
            say(&format!("  {}", render(member)));
        }
    }
    code
}
