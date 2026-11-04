//! Applies a poll's decisions in one write transaction: the heartbeat, the
//! moves of results, and the announcements. A line is only returned for
//! what the store now says was announced, so a line is never printed twice
//! and never printed before it is stored (INV-9). An event that would push
//! the line past `LINE_MAX` is not applied at all: the next poll plans it
//! again, and `TEAM-COMPLETE` waits until nothing before it is held back.

use std::time::Duration;

use serde_json::json;
use teamwire_core::{EventType, Pid, Timestamp};
use teamwire_store::{Ctx, StoreError, Tx};

use crate::plan::{Action, Plan};
use crate::text::{LINE_MAX, SEP, STALLS_ENDED, event};

/// One event that goes into the line, with its
/// `[teamwire:<team> HH:MM:SSZ]` prefix.
pub(crate) struct Emitted {
    pub name: &'static str,
    pub text: String,
}

/// The text an action prints if it is applied; `None` prints nothing.
fn printed(action: &Action) -> Option<&str> {
    match action {
        Action::Announce { text, .. }
        | Action::ViaWatch { text, .. }
        | Action::Interrupt { text, .. } => Some(text),
        Action::EndStall => None,
    }
}

/// The line's length, in characters, once `next` is joined to `emitted`.
fn joined_len(emitted: &[Emitted], next: &str) -> usize {
    emitted
        .iter()
        .map(|e| e.text.chars().count() + SEP.len())
        .sum::<usize>()
        + next.chars().count()
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
    team: &str,
    out: &mut Vec<Emitted>,
) -> Result<(), StoreError> {
    match action {
        Action::Announce { key, name, text } => {
            if tx.announce_once(at, &key)? {
                out.push(Emitted {
                    name,
                    text: event(team, at, &text),
                });
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
                    text: event(team, at, &text),
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
                    text: event(team, at, &text),
                });
            }
        }
        Action::EndStall => {
            tx.next_counter(STALLS_ENDED)?;
        }
    }
    Ok(())
}

/// Runs `plan`. `watching` puts `WATCHING n members` first (the first poll),
/// and the plan's `TEAM-COMPLETE` goes last. The `watch.line` event is
/// written in the same transaction when anything is printed.
///
/// Actions print in plan order until the next one would not fit in the
/// line with room left for `TEAM-COMPLETE`. That one and every later one
/// that prints are left unapplied, so their keys stay unannounced and their
/// results unmoved, and `TEAM-COMPLETE` is left for a later poll too.
pub(crate) fn apply(
    ctx: &mut Ctx<'_>,
    plan: Plan,
    team: &str,
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
        let complete = complete.map(|text| event(team, at, &text));
        let room = LINE_MAX
            - complete
                .as_ref()
                .map_or(0, |c| c.chars().count() + SEP.len());
        let mut emitted = Vec::new();
        if let Some(n) = watching {
            emitted.push(Emitted {
                name: "WATCHING",
                text: event(team, at, &format!("WATCHING {n} members")),
            });
        }
        let mut held = false;
        for action in actions {
            if let Some(text) = printed(&action) {
                held = held || joined_len(&emitted, &event(team, at, text)) > room;
                if held {
                    continue;
                }
            }
            apply_one(tx, at, stale, action, team, &mut emitted)?;
        }
        if let Some(text) = complete.filter(|_| !held) {
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
