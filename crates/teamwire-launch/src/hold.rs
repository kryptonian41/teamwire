//! The sleep hold (SLP-1, SLP-2): the Mac must not sleep while the team
//! works, and the hold waits on the lead's real harness process, never on
//! the shell that ran `teamwire`.

use std::time::Duration;

use teamwire_core::{HarnessId, HoldInfo, Pid, Registry, SessionId, TeamName, TeamState};
use teamwire_store::{Ctx, Store};

const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// Starts or reuses the hold and records it on the team. Every way this can
/// go wrong is said in `warnings`, because a missing hold is a laptop that
/// sleeps under a running team.
pub fn ensure_hold(
    ctx: &mut Ctx<'_>,
    team: &TeamName,
    lead: (&HarnessId, Option<&SessionId>),
    warnings: &mut Vec<String>,
) {
    let Some(hold) = ctx.platform.sleep_hold() else {
        warnings.push(
            "no sleep hold on this platform: the machine may sleep under the team".to_owned(),
        );
        return;
    };
    let (harness, Some(session)) = lead else {
        return;
    };
    let Some(lead_pid) = lead_process(ctx.registry, harness, session, warnings) else {
        return;
    };
    let mine = team_of(ctx.store).and_then(|t| t.sleep_hold);
    let info = if let Some(found) = hold.find_existing(lead_pid) {
        // A process list cannot say who started a hold, so `found.owned` is
        // not believed: this team owns it only if a team already recorded so.
        let owned = mine.is_some_and(|m| m.pid == found.pid && m.owned)
            || another_team_owns(ctx, team, session, found.pid);
        HoldInfo {
            pid: found.pid,
            owned,
        }
    } else {
        match hold.start(lead_pid) {
            Ok(info) => HoldInfo {
                pid: info.pid,
                owned: true,
            },
            Err(e) => {
                warnings.push(format!(
                    "the sleep hold could not be started ({e}): the machine may sleep under the team"
                ));
                return;
            }
        }
    };
    if let Err(e) = ctx.store.write(|tx| tx.set_sleep_hold(Some(&info))) {
        warnings.push(format!(
            "the sleep hold runs (pid {}) but could not be recorded: {e}",
            info.pid.get()
        ));
    }
}

/// The lead's harness process, found from the lead's session. Every way it
/// can be missing is said in `warnings`.
pub fn lead_process(
    registry: &Registry,
    harness: &HarnessId,
    session: &SessionId,
    warnings: &mut Vec<String>,
) -> Option<Pid> {
    let found = registry
        .harness(harness)
        .map_err(|e| e.to_string())
        .and_then(|h| {
            h.processes_for_session(session, LIST_TIMEOUT)
                .map_err(|e| e.to_string())
        });
    match found {
        Ok(pids) => {
            let first = pids.iter().min().copied();
            if first.is_none() {
                warnings.push(format!(
                    "NO SLEEP HOLD: no process of the lead's session {session} was found, so nothing \
                     was held; the machine may sleep under the team"
                ));
            } else if pids.len() > 1 {
                warnings.push(format!(
                    "the lead's session {session} has {} processes; the sleep hold waits on the oldest",
                    pids.len()
                ));
            }
            first
        }
        Err(e) => {
            warnings.push(format!(
                "NO SLEEP HOLD: the lead's process could not be looked up ({e}); the machine may \
                 sleep under the team"
            ));
            None
        }
    }
}

/// True when another open team of the same lead records this hold as its own.
fn another_team_owns(ctx: &Ctx<'_>, this: &TeamName, session: &SessionId, hold: Pid) -> bool {
    let Ok(names) = teamwire_store::team_names(ctx.home) else {
        return false;
    };
    names.into_iter().any(|name| {
        if &name == this {
            return false;
        }
        let Ok(store) = Store::open_read_only(ctx.home, &name) else {
            return false;
        };
        team_of(&store).is_some_and(|t| {
            t.state == TeamState::Open
                && t.lead_session.as_ref() == Some(session)
                && t.sleep_hold
                    == Some(HoldInfo {
                        pid: hold,
                        owned: true,
                    })
        })
    })
}

/// The team row, if the store has one and it reads.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn team_of(store: &Store) -> Option<teamwire_store::TeamRow> {
    store.read(|tx| tx.team()).ok().flatten()
}
