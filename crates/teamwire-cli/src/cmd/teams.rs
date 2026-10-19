//! `teamwire teams`: every team on this machine with its state and
//! members, so an agent can find who to message in another team. Reads
//! only, and needs no proof: the roster holds no place, token or session id.

use std::process::ExitCode;

use serde::Serialize;
use serde_json::json;
use teamwire_core::{TicketRef, Timestamp};
use teamwire_msg::{TeamEntry, describe};

use crate::cmd::members::render;
use crate::cmd::send::{STORE, fail, home_or_refuse, say, warn, with_read_ctx};
use crate::wiring::Wiring;
use crate::{Global, TeamsArgs};

/// ` (refs linear:GKA-580, github:#1)`, or nothing for a team without refs.
pub(crate) fn refs_suffix(refs: &[TicketRef]) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = refs.iter().map(ToString::to_string).collect();
    format!(" (refs {})", shown.join(", "))
}

/// A team's roster and, for an AFK run, when its budget ends.
#[derive(Serialize)]
struct Row {
    #[serde(flatten)]
    entry: TeamEntry,
    afk_budget_ends_at: Option<Timestamp>,
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn read_row(ctx: &mut teamwire_store::Ctx<'_>) -> Result<Row, String> {
    let entry = describe(ctx).map_err(|e| e.to_string())?;
    let team = ctx.store.read(|tx| tx.team()).map_err(|e| e.to_string())?;
    Ok(Row {
        entry,
        afk_budget_ends_at: team.and_then(|t| t.afk_budget_ends_at),
    })
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
    let mut entries: Vec<Row> = Vec::new();
    let mut code = ExitCode::SUCCESS;
    for name in &names {
        match with_read_ctx(global, wiring, name, read_row) {
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
    for Row {
        entry,
        afk_budget_ends_at,
    } in &entries
    {
        let marker = if afk_budget_ends_at.is_some() {
            ", afk"
        } else {
            ""
        };
        say(&format!(
            "{} ({}{marker}) {}: address as lead@{} or <role>@{}{}\n",
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
