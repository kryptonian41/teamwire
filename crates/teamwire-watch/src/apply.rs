//! Applies a poll's decisions in one write transaction: the heartbeat, the
//! moves of results, and the announcements. A line is only returned for
//! what the store now says was announced, so a line is never printed twice
//! and never printed before it is stored (INV-9).

use std::time::Duration;

use serde_json::json;
use teamwire_core::{EventType, Pid, Timestamp};
use teamwire_store::{Ctx, StoreError, Tx};

use crate::plan::{Action, Plan};

/// One event that goes into the line.
pub(crate) struct Emitted {
    pub name: &'static str,
    pub text: String,
}

pub(crate) struct Applied {
    pub emitted: Vec<Emitted>,
    /// The lease is no longer this watch's: nothing was applied.
    pub lease_lost: bool,
}

fn apply_one(
    tx: &Tx<'_>,
    at: Timestamp,
    stale: Duration,
    action: Action,
    out: &mut Vec<Emitted>,
) -> Result<(), StoreError> {
    match action {
        Action::Announce { key, name, text } => {
            if tx.announce_once(at, &key)? {
                out.push(Emitted { name, text });
            }
        }
        Action::ViaWatch {
            completion,
            key,
            text,
        } => {
            if tx.deliver_completion_via_watch(at, &completion)? {
                tx.announce_once(at, &key)?;
                out.push(Emitted {
                    name: "COMPLETED",
                    text,
                });
            }
        }
        Action::Interrupt {
            completion,
            key,
            text,
        } => {
            if tx.interrupt_stale_delivery(at, &completion, stale)? {
                tx.announce_once(at, &key)?;
                out.push(Emitted {
                    name: "UNDELIVERED",
                    text,
                });
            }
        }
        Action::Say { name, text } => out.push(Emitted { name, text }),
    }
    Ok(())
}

/// Runs `plan`. `watching` puts `WATCHING n members` first (the first poll),
/// and the plan's `TEAM-COMPLETE` goes last. The `watch.line` event is
/// written in the same transaction when anything is printed.
pub(crate) fn apply(
    ctx: &mut Ctx<'_>,
    plan: Plan,
    lease: Option<Pid>,
    watching: Option<usize>,
    at: Timestamp,
) -> Result<Applied, StoreError> {
    let stale = Duration::from_secs(ctx.config.delivering_stale_s);
    let Plan { actions, complete } = plan;
    ctx.store.write(|tx| {
        if let Some(pid) = lease
            && !tx.heartbeat_watch_lease(at, pid)?
        {
            return Ok(Applied {
                emitted: Vec::new(),
                lease_lost: true,
            });
        }
        let mut emitted = Vec::new();
        if let Some(n) = watching {
            emitted.push(Emitted {
                name: "WATCHING",
                text: format!("WATCHING {n} members"),
            });
        }
        for action in actions {
            apply_one(tx, at, stale, action, &mut emitted)?;
        }
        if let Some(text) = complete {
            emitted.push(Emitted {
                name: "TEAM-COMPLETE",
                text,
            });
        }
        if !emitted.is_empty() {
            let names: Vec<&str> = emitted.iter().map(|e| e.name).collect();
            tx.record_event(
                at,
                EventType::WatchLine,
                None,
                None,
                &json!({ "events": names }),
            )?;
        }
        Ok(Applied {
            emitted,
            lease_lost: false,
        })
    })
}
