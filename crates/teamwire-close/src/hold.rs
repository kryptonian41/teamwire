//! Releasing the sleep hold (SLP-1, INV-26): only a hold this team started,
//! only when no other open team still records it, and only after the platform
//! has re-checked that the process is still that hold on the lead.

use std::time::Duration;

use teamwire_core::{Pid, ReleaseOutcome, TeamName, TeamState};
use teamwire_store::{Ctx, Party, Store, StoreError, TeamRow};

const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// Releases the team's hold if it may, and says what it did. `Err` is a hold
/// that could not be released for a reason the lead must see; leaving a hold
/// running because someone else needs it, or because it is not ours, is `Ok`.
/// The record is cleared once the hold is gone, so what the store says is
/// what runs.
pub(crate) fn release(ctx: &mut Ctx<'_>, team: &TeamRow) -> Result<String, String> {
    let Some(hold) = team.sleep_hold else {
        return Ok("no sleep hold is recorded".to_owned());
    };
    if !hold.owned {
        return Ok(format!(
            "sleep hold {} left running: teamwire did not start it",
            hold.pid
        ));
    }
    match another_team_records(ctx, &team.name, hold.pid) {
        Ok(Some(other)) => {
            return Ok(format!(
                "sleep hold {} left running: team {other} still records it",
                hold.pid
            ));
        }
        Ok(None) => {}
        Err(why) => {
            return Ok(format!(
                "sleep hold {} left running: {why}, so it may still be needed",
                hold.pid
            ));
        }
    }
    let Some(platform_hold) = ctx.platform.sleep_hold() else {
        return Ok("this platform has no sleep hold to release".to_owned());
    };
    let Some(lead) = lead_pid(ctx, team) else {
        return Ok(format!(
            "sleep hold {} left running: the lead's process was not found, and the hold ends \
             when the lead exits",
            hold.pid
        ));
    };
    let outcome = platform_hold
        .release(&hold, lead)
        .map_err(|e| format!("the sleep hold {} could not be released: {e}", hold.pid))?;
    ctx.store
        .write(|tx| tx.set_sleep_hold(None))
        .map_err(|e| format!("the released hold's record could not be cleared: {e}"))?;
    Ok(match outcome {
        ReleaseOutcome::Released => format!("released sleep hold {}", hold.pid),
        ReleaseOutcome::AlreadyGone => format!("sleep hold {} had already ended", hold.pid),
        ReleaseOutcome::NotOwned => format!("sleep hold {} was not ours; left running", hold.pid),
        ReleaseOutcome::NotAHold => format!(
            "pid {} is no longer a sleep hold on the lead; left alone",
            hold.pid
        ),
    })
}

/// The lead's own harness process, found from the lead's session.
fn lead_pid(ctx: &Ctx<'_>, team: &TeamRow) -> Option<Pid> {
    let session = team.lead_session.as_ref()?;
    let (harness, _) = ctx.adapters(&Party::Lead).ok()?;
    let pids = harness.processes_for_session(session, LIST_TIMEOUT).ok()?;
    pids.into_iter().min()
}

/// The name of another open team that records this hold, if there is one. A
/// team whose store cannot be read is an error: it might be the one that
/// needs the hold.
fn another_team_records(
    ctx: &Ctx<'_>,
    this: &TeamName,
    hold: Pid,
) -> Result<Option<TeamName>, String> {
    let Ok(entries) = std::fs::read_dir(ctx.home.join("teams")) else {
        return Ok(None);
    };
    for entry in entries.flatten() {
        let Ok(name) = TeamName::parse(&entry.file_name().to_string_lossy()) else {
            continue;
        };
        if &name == this {
            continue;
        }
        let store = match Store::open_read_only(ctx.home, &name) {
            Ok(store) => store,
            // A directory with no database is not a team that needs a hold.
            Err(StoreError::NoDatabase { .. }) => continue,
            Err(e) => return Err(format!("team {name} could not be read ({e})")),
        };
        let team = read_team(&store).map_err(|e| format!("team {name} could not be read ({e})"))?;
        if team.state == TeamState::Open && team.sleep_hold.is_some_and(|h| h.pid == hold) {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn read_team(store: &Store) -> Result<TeamRow, String> {
    store
        .read(|tx| tx.team())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no team row".to_owned())
}
