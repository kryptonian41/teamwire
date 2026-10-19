//! Finding other teams (MSG-21): every team under the home with its state
//! and roster, and one other team's roster. Each team's store is opened
//! read-only, so a look changes nothing anywhere.

use serde::{Deserialize, Serialize};
use teamwire_core::{Refusal, TeamName, TeamState, TicketRef};
use teamwire_store::{Ctx, Store, StoreError, team_names};

use crate::send::read_team;
use crate::{RosterEntry, SendError, roster};

/// One team as `team_teams` and `teamwire teams` show it. Its agents are
/// addressed as `lead@<name>` and `<role>@<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamEntry {
    pub name: TeamName,
    pub title: String,
    /// Only an open team takes messages.
    pub state: TeamState,
    /// The tickets the team works on; empty when it has none.
    pub refs: Vec<TicketRef>,
    pub members: Vec<RosterEntry>,
}

/// Every team under the home, by name. A team whose store cannot be read is
/// left out and logged: one damaged team must not hide the others.
pub fn teams(ctx: &Ctx<'_>) -> Result<Vec<TeamEntry>, SendError> {
    let mut out = Vec::new();
    for name in team_names(ctx.home)? {
        match entry(ctx, &name) {
            Ok(entry) => out.push(entry),
            Err(e) => tracing::warn!(team = %name, error = %e, "a team is left out of the list"),
        }
    }
    Ok(out)
}

/// The roster of `team`, which may be the caller's own or another.
pub fn roster_of(ctx: &Ctx<'_>, team: &TeamName) -> Result<Vec<RosterEntry>, SendError> {
    Ok(entry(ctx, team)?.members)
}

/// The team of `ctx`'s own store, as `teams` lists it.
pub fn describe(ctx: &Ctx<'_>) -> Result<TeamEntry, SendError> {
    let row = read_team(ctx)?.ok_or(Refusal::UnknownTeam)?;
    Ok(TeamEntry {
        name: row.name,
        title: row.title,
        state: row.state,
        refs: row.refs,
        members: roster(ctx)?,
    })
}

fn entry(ctx: &Ctx<'_>, team: &TeamName) -> Result<TeamEntry, SendError> {
    let mut store = match Store::open_read_only(ctx.home, team) {
        Ok(store) => store,
        Err(StoreError::NoDatabase { .. }) => return Err(Refusal::UnknownTeam.into()),
        Err(e) => return Err(e.into()),
    };
    describe(&Ctx {
        home: ctx.home,
        store: &mut store,
        registry: ctx.registry,
        platform: ctx.platform,
        clock: ctx.clock,
        config: ctx.config,
    })
}
