//! One watch per team (WAT-16): the lease row, taken at start and released
//! at the end.

use serde_json::json;
use teamwire_core::Pid;
use teamwire_store::{Ctx, LeaseOutcome, StoreError};
use thiserror::Error;

use crate::memory::WatchMemory;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WatchError {
    #[error("another watch already runs for this team: pid {pid} on {host}")]
    Held { pid: Pid, host: String },
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Takes the team's watch lease for `pid`. A lease held by a live process
/// refuses; one held by a dead process is taken over. Every later poll
/// refreshes the lease's heartbeat.
pub fn start(
    ctx: &mut Ctx<'_>,
    memory: &mut WatchMemory,
    pid: Pid,
    host: &str,
) -> Result<(), WatchError> {
    let at = ctx.clock.now();
    let platform = ctx.platform;
    let outcome = ctx
        .store
        .write(|tx| tx.take_watch_lease(at, pid, host, |held| !platform.pid_alive(held.pid)))?;
    match outcome {
        LeaseOutcome::Taken => {
            memory.lease_pid = Some(pid);
            Ok(())
        }
        LeaseOutcome::HeldBy(held) => Err(WatchError::Held {
            pid: held.pid,
            host: held.host,
        }),
    }
}

/// Gives the lease up and records why the watch ended. Does nothing when
/// this watch does not hold the lease.
pub fn stop(ctx: &mut Ctx<'_>, memory: &mut WatchMemory) -> Result<(), StoreError> {
    let Some(pid) = memory.lease_pid.take() else {
        return Ok(());
    };
    let at = ctx.clock.now();
    let reason = memory.end.map_or("interrupted", |end| match end {
        crate::WatchEnd::TeamComplete => "team_complete",
        crate::WatchEnd::TeamClosed => "team_closed",
        crate::WatchEnd::StoreError => "store_error",
        crate::WatchEnd::LeaseLost => "lease_lost",
    });
    let data = json!({ "reason": reason, "polls": memory.polls });
    ctx.store
        .write(|tx| tx.release_watch_lease(at, pid, &data))?;
    Ok(())
}
