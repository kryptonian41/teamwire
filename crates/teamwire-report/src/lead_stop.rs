//! The lead's Stop hook (ADR 0018 point 5): which teams are working with
//! nobody watching them.

use std::path::Path;

use teamwire_core::{Clock, MemberState, Platform, SessionId, TeamName, TeamState};
use teamwire_deliver::watch_lease_is_dead;
use teamwire_store::{ReadTx, Store, StoreError, team_names};

/// The teams under `home` that the lead of `session` should be watching and
/// is not. A team counts when it is open, `session` is its lead session, a
/// started member has no completion for its current generation, and no live
/// watch lease exists. Each team's store is opened read-only. A team that
/// cannot be read is left out and logged, so a damaged team never stops the
/// lead from stopping.
pub fn teams_without_watch(
    home: &Path,
    session: &SessionId,
    platform: &dyn Platform,
    clock: &dyn Clock,
) -> Vec<TeamName> {
    let names = team_names(home).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "the teams could not be listed");
        Vec::new()
    });
    names
        .into_iter()
        .filter(|name| match needs_watch(home, name, session, platform, clock) {
            Ok(needs) => needs,
            Err(e) => {
                tracing::warn!(team = %name, error = %e, "a team is left out of the watch check");
                false
            }
        })
        .collect()
}

fn needs_watch(
    home: &Path,
    team: &TeamName,
    session: &SessionId,
    platform: &dyn Platform,
    clock: &dyn Clock,
) -> Result<bool, StoreError> {
    let store = Store::open_read_only(home, team)?;
    let now = clock.now();
    store.read(|tx| {
        let Some(row) = tx.team()? else {
            return Ok(false);
        };
        if row.state != TeamState::Open || row.lead_session.as_ref() != Some(session) {
            return Ok(false);
        }
        // A lease is cheap to read and rules most teams out first.
        if tx
            .watch_lease()?
            .is_some_and(|lease| !watch_lease_is_dead(&lease, platform, now))
        {
            return Ok(false);
        }
        has_working_member(tx)
    })
}

/// Whether a started member has no completion for its current generation.
fn has_working_member(tx: &ReadTx<'_>) -> Result<bool, StoreError> {
    for member in tx.members()?.items {
        if member.state != MemberState::Started {
            continue;
        }
        let done = tx
            .latest_completion(&member.role)?
            .is_some_and(|r| r.completion.generation == member.generation);
        if !done {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether an open team led by `session` has a working member and a live
/// watch, so its watch will wake the lead.
fn wakes_lead(
    home: &Path,
    team: &TeamName,
    session: &SessionId,
    platform: &dyn Platform,
    clock: &dyn Clock,
) -> Result<bool, StoreError> {
    let store = Store::open_read_only(home, team)?;
    let now = clock.now();
    store.read(|tx| {
        let Some(row) = tx.team()? else {
            return Ok(false);
        };
        if row.state != TeamState::Open || row.lead_session.as_ref() != Some(session) {
            return Ok(false);
        }
        let watched = tx
            .watch_lease()?
            .is_some_and(|lease| !watch_lease_is_dead(&lease, platform, now));
        Ok(watched && has_working_member(tx)?)
    })
}

/// The AFK notes for the open AFK teams that `session` leads, when nothing
/// will wake the lead: no open team the session leads has both a working
/// member and a live watch lease. A team that cannot be read gets no note,
/// does not count as waking the lead, and is logged, as above.
pub fn afk_hold_notes(
    home: &Path,
    session: &SessionId,
    platform: &dyn Platform,
    clock: &dyn Clock,
) -> Vec<String> {
    let names = team_names(home).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "the teams could not be listed");
        Vec::new()
    });
    let woken = names.iter().any(|name| {
        wakes_lead(home, name, session, platform, clock).unwrap_or_else(|e| {
            tracing::warn!(team = %name, error = %e, "a team is left out of the wake check");
            false
        })
    });
    if woken {
        return Vec::new();
    }
    names
        .into_iter()
        .filter_map(|name| match afk_note_for(home, &name, session, clock) {
            Ok(note) => note,
            Err(e) => {
                tracing::warn!(team = %name, error = %e, "a team is left out of the AFK hold");
                None
            }
        })
        .collect()
}

fn afk_note_for(
    home: &Path,
    team: &TeamName,
    session: &SessionId,
    clock: &dyn Clock,
) -> Result<Option<String>, StoreError> {
    let store = Store::open_read_only(home, team)?;
    let now = clock.now();
    store.read(|tx| {
        let Some(row) = tx.team()? else {
            return Ok(None);
        };
        let Some(ends) = row.afk_budget_ends_at else {
            return Ok(None);
        };
        if row.state != TeamState::Open || row.lead_session.as_ref() != Some(session) {
            return Ok(None);
        }
        let dir = home.join("teams").join(team.as_str()).display().to_string();
        // Close accepts a team that never had a member without a report, so
        // telling that lead to add a reporter first would be wrong advice.
        let never_had_a_member = tx.members()?.all_or_corrupt().is_ok_and(|m| m.is_empty());
        Ok(Some(if never_had_a_member {
            format!(
                "[teamwire:{team}] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to {dir}/afk/decisions.md, and carry on. The team has no members yet: add the ones the task needs, or, if the run is over, run `teamwire close {team}`, which needs no report for a team that never had a member. If a command you started will wake you, stop again."
            )
        } else if now >= ends {
            format!(
                "[teamwire:{team}] budget spent at {ends}: finish what is running, write {dir}/afk/REPORT.md, then run `teamwire close {team}`. Start no new implementer. If a command you started will wake you, stop again."
            )
        } else {
            format!(
                "[teamwire:{team}] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to {dir}/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close {team}` once {dir}/afk/REPORT.md exists. If a command you started will wake you, stop again."
            )
        }))
    })
}

/// How the lead starts the watch. A lead that cannot read a command's output
/// as it is printed (ADR 0023) runs it in the background, and the watch types
/// each line into the lead's pane.
pub(crate) fn watch_command(team: &TeamName, reads_watch_lines: bool) -> String {
    if reads_watch_lines {
        format!("teamwire watch {team}")
    } else {
        format!("nohup teamwire watch {team} --deliver >/dev/null 2>&1 &")
    }
}

/// The line the hold carries for one team. `reads_watch_lines` is the lead's
/// harness capability of the same name.
#[must_use]
pub fn watch_missing_note(team: &TeamName, reads_watch_lines: bool) -> String {
    let command = watch_command(team, reads_watch_lines);
    if reads_watch_lines {
        format!(
            "[teamwire:{team}] no watch is running; start `{command}` with Monitor before you stop."
        )
    } else {
        format!(
            "[teamwire:{team}] no watch is running; start it in the background with `{command}` before you stop."
        )
    }
}
