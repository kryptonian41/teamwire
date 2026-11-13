//! The lead's Stop hook (ADR 0018 point 5): which teams are working with
//! nobody watching them.

use std::path::Path;

use teamwire_core::harness::WatchStart;
use teamwire_core::{Clock, MemberState, Platform, SessionId, TeamName, Timestamp};
use teamwire_deliver::{WatchText, watch_lease_is_dead, watch_text};
use teamwire_store::{OpenTeam, ReadTx, StoreError, TeamRead, open_teams};

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
    let now = clock.now();
    led_by(home, session)
        .into_iter()
        .filter_map(|(name, read)| match needs_watch(read, platform, now) {
            Ok(needs) => needs.then_some(name),
            Err(e) => {
                tracing::warn!(team = %name, error = %e, "a team is left out of the watch check");
                None
            }
        })
        .collect()
}

/// The open teams `session` leads, each with what its read gave; none when
/// the teams cannot be listed, which is logged.
pub(crate) fn led_by(home: &Path, session: &SessionId) -> Vec<(TeamName, TeamRead)> {
    open_teams(home, Some(session)).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "the teams could not be listed");
        Vec::new()
    })
}

fn needs_watch(
    read: TeamRead,
    platform: &dyn Platform,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let Some(team) = read? else {
        return Ok(false);
    };
    team.store.read(|tx| {
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
    team: &OpenTeam,
    platform: &dyn Platform,
    now: Timestamp,
) -> Result<bool, StoreError> {
    team.store.read(|tx| {
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
    let now = clock.now();
    let teams = led_by(home, session);
    let woken = teams.iter().any(|(name, read)| {
        let woken = match read {
            Ok(Some(team)) => wakes_lead(team, platform, now).map_err(|e| e.to_string()),
            Ok(None) => Ok(false),
            Err(e) => Err(e.to_string()),
        };
        woken.unwrap_or_else(|e| {
            tracing::warn!(team = %name, error = %e, "a team is left out of the wake check");
            false
        })
    });
    if woken {
        return Vec::new();
    }
    teams
        .into_iter()
        .filter_map(|(name, read)| match afk_note_for(home, &name, read, now) {
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
    read: TeamRead,
    now: Timestamp,
) -> Result<Option<String>, StoreError> {
    let Some(open) = read? else {
        return Ok(None);
    };
    let Some(ends) = open.row.afk_budget_ends_at else {
        return Ok(None);
    };
    open.store.read(|tx| {
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
                "[teamwire:{team}] budget spent at {ends}: start no new implementer. Finish what is running, add the reporter if it has not run, then run `teamwire close {team}` once {dir}/afk/REPORT.md has every heading. If a command you started will wake you, stop again."
            )
        } else {
            format!(
                "[teamwire:{team}] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to {dir}/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close {team}` once {dir}/afk/REPORT.md exists. If a command you started will wake you, stop again."
            )
        }))
    })
}

/// The line the hold carries for one team. `start` is how the lead's harness
/// starts a watch.
#[must_use]
pub fn watch_missing_note(team: &TeamName, start: WatchStart) -> String {
    watch_text(start, WatchText::StopHold, team)
}

/// The lines the lead's hooks add to the lead's turn for each item that was
/// never typed into the lead's pane (ADR 0043 decision 1).
pub fn undelivered_to_lead(
    _home: &Path,
    _session: &SessionId,
    _platform: &dyn Platform,
    _clock: &dyn Clock,
) -> Vec<String> {
    Vec::new()
}
