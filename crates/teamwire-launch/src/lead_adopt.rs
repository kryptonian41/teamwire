//! Step 6 of a lead handoff (ADR 0051): the new lead becomes each led
//! team's lead, recorded by the command.
//!
//! Never: roll back a team that moved; record a team that closed or took
//! another lead since the plan.

use std::path::Path;

use teamwire_core::{Place, SessionId, SessionKind, TeamName, TeamState};
use teamwire_store::{Store, StoreError};

use crate::handoff::HandoffError;
use crate::lead_plan::{LeadHandoffCtx, Planned};

/// Step 6: one store write per team. A team that fails stays with the old
/// lead, and the teams before it keep the new one.
pub(crate) fn adopt_all(
    ctx: &LeadHandoffCtx<'_>,
    planned: &Planned<'_>,
    new: &SessionId,
    new_place: &Place,
) -> Result<Vec<(TeamName, SessionId)>, HandoffError> {
    let old = &planned.seat.session;
    let mut moved = Vec::new();
    for (i, team) in planned.seat.names().into_iter().enumerate() {
        match adopt_one(ctx, &team, old, new, &planned.split.cwd) {
            Ok(()) => moved.push((team, new.clone())),
            Err(why) => {
                let rest: Vec<TeamName> = planned.seat.names().into_iter().skip(i).collect();
                return Err(partial_adoption(
                    planned,
                    new_place,
                    &moved,
                    &rest,
                    &format!("team {team}: {why}"),
                ));
            }
        }
    }
    Ok(moved)
}

fn adopt_one(
    ctx: &LeadHandoffCtx<'_>,
    team: &TeamName,
    old: &SessionId,
    new: &SessionId,
    cwd: &Path,
) -> Result<(), String> {
    let mut store = Store::open_existing(ctx.home, team).map_err(|e| e.to_string())?;
    let at = ctx.clock.now();
    let written = store.write(|tx| {
        // The team may have closed, or adopted another lead, since the plan.
        let Some(row) = tx.reader().team()? else {
            return Ok(Err("it has no record".to_owned()));
        };
        let still_led = row.state == TeamState::Open && row.lead_session.as_ref() == Some(old);
        if !still_led {
            return Ok(Err(format!(
                "it is {} and its lead is {}",
                row.state,
                row.lead_session
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), ToString::to_string)
            )));
        }
        tx.adopt_lead_in(at, new, SessionKind::Interactive, Some(cwd))
            .map(|_| Ok(()))
    });
    written.map_err(|e: StoreError| e.to_string())?
}

fn partial_adoption(
    planned: &Planned<'_>,
    new_place: &Place,
    moved: &[(TeamName, SessionId)],
    rest: &[TeamName],
    why: &str,
) -> HandoffError {
    let names = |teams: &mut dyn Iterator<Item = &TeamName>| {
        let list: Vec<String> = teams.map(ToString::to_string).collect();
        if list.is_empty() {
            "none".to_owned()
        } else {
            list.join(", ")
        }
    };
    let commands: Vec<String> = rest
        .iter()
        .map(|t| format!("`teamwire lead --adopt {t}`"))
        .collect();
    HandoffError::Partial(format!(
        "{why}. Moved to the new lead: {}. Still led by this session: {}. Both panes stay open: \
         this one ({}) and the new lead's ({}). The new lead takes the rest with {}",
        names(&mut moved.iter().map(|(t, _)| t)),
        names(&mut rest.iter()),
        planned.old_place.id,
        new_place.id,
        commands.join(", ")
    ))
}

/// Each led team whose recorded lead is no longer the old session, with that
/// lead, read without a write.
pub(crate) fn moved_teams(
    ctx: &LeadHandoffCtx<'_>,
    planned: &Planned<'_>,
) -> Vec<(TeamName, SessionId)> {
    let old = &planned.seat.session;
    planned
        .seat
        .names()
        .into_iter()
        .filter_map(|t| lead_of(ctx.home, &t).filter(|l| l != old).map(|l| (t, l)))
        .collect()
}

/// The lead session a team records now, read without a write.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn lead_of(home: &Path, team: &TeamName) -> Option<SessionId> {
    let store = Store::open_read_only(home, team).ok()?;
    store.read(|tx| tx.team()).ok().flatten()?.lead_session
}
