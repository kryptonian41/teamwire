//! The lead's Stop hook (ADR 0018 point 5): which teams are working with
//! nobody watching them.

use std::path::Path;

use teamwire_core::{Clock, MemberState, Platform, SessionId, TeamName, TeamState};
use teamwire_deliver::watch_lease_is_dead;
use teamwire_store::{Store, StoreError, team_names};

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
    })
}

/// The line the hold carries for one team.
#[must_use]
pub fn watch_missing_note(team: &TeamName) -> String {
    format!(
        "[teamwire:{team}] no watch is running; start `teamwire watch {team}` with Monitor before you stop."
    )
}
