//! `teamwire members`: the team roster.

use std::process::ExitCode;

use serde_json::json;
use teamwire_core::TeamName;
use teamwire_msg::{RosterEntry, roster};

use crate::cmd::send::error_code;
use crate::cmd::shared::{fail, say, with_read_ctx};
use crate::wiring::Wiring;
use crate::{Global, MembersArgs, USAGE};

pub(crate) fn run(args: &MembersArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let MembersArgs { team, json } = args;
    let name = team.clone().or_else(|| std::env::var("TEAMWIRE_TEAM").ok());
    let Some(team) = name.and_then(|t| TeamName::parse(&t).ok()) else {
        return fail("members", "name a team, or set TEAMWIRE_TEAM", USAGE);
    };
    // The roster holds no place, token or session id, so reading it needs no proof.
    match with_read_ctx(global, wiring, &team, |ctx| roster(ctx)) {
        Err(code) => code,
        Ok(Err(e)) => fail("members", &e.to_string(), error_code(&e)),
        Ok(Ok(entries)) => {
            if *json {
                say(&format!("{}\n", json!({ "members": entries })));
            } else {
                for entry in &entries {
                    say(&render(entry));
                }
            }
            ExitCode::SUCCESS
        }
    }
}

pub(crate) fn render(e: &RosterEntry) -> String {
    let accepts: Vec<&str> = e.accepts_from_members.iter().map(|t| t.as_str()).collect();
    let wake = if e.can_be_typed_to {
        "typed"
    } else {
        "inbox only"
    };
    format!(
        "{:<16} {:<12} {:<8} gen {} [{wake}] from members {}: {}\n",
        e.role,
        e.kind,
        e.lifecycle,
        e.generation,
        if accepts.is_empty() {
            "none (lead only)".to_owned()
        } else {
            accepts.join(",")
        },
        e.title
    )
}
