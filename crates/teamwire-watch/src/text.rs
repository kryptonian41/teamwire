//! The announcement keys and the wording of each event (`docs/api/cli.md`
//! "`teamwire watch`"). A key names one occurrence, so it carries the
//! generation or episode number that makes the next occurrence a new key.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Role, Ulid, one_line};

pub(crate) use teamwire_deliver::agent_text;

/// The longest one event may be. A poll joins several, so each is cut alone
/// and a long summary cannot push a later event out of the line.
const EVENT_MAX: usize = 300;
/// The longest a whole line may be.
pub(crate) const LINE_MAX: usize = 2000;

pub(crate) fn silent_prefix(role: &Role, generation: u32) -> String {
    format!("SILENT:{role}:{generation}:")
}

pub(crate) fn silent_key(role: &Role, generation: u32, episode: u32) -> String {
    format!("{}{episode}", silent_prefix(role, generation))
}

pub(crate) fn blocked_prefix(role: &Role, generation: u32) -> String {
    format!("BLOCKED:{role}:{generation}:")
}

pub(crate) fn blocked_key(role: &Role, generation: u32, episode: u32) -> String {
    format!("{}{episode}", blocked_prefix(role, generation))
}

/// An announcement key `TEAM-STALLED:<n>` names the stall episode `n`.
pub(crate) const STALLED_PREFIX: &str = "TEAM-STALLED:";
/// The counter of ended stall episodes. It is a counter and not an
/// announcement because the end of an episode is never printed.
pub(crate) const STALLS_ENDED: &str = "watch.stalls_ended";

pub(crate) fn stalled_key(episode: u32) -> String {
    format!("{STALLED_PREFIX}{episode}")
}

pub(crate) fn late_key(role: &Role, generation: u32) -> String {
    format!("LATE:{role}:{generation}")
}

pub(crate) fn died_key(role: &Role, generation: u32) -> String {
    format!("DIED:{role}:{generation}")
}

pub(crate) fn result_key(kind: &str, completion: &Ulid) -> String {
    format!("{kind}:{completion}")
}

pub(crate) fn question_key(kind: &str, message: &Ulid) -> String {
    format!("{kind}:{message}")
}

/// `31m`, `2h05m` or `45s`: an age as the lead reads it.
pub(crate) fn age(span: Duration) -> String {
    let s = span.as_secs();
    if s >= 3600 {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m", s / 60)
    } else {
        format!("{s}s")
    }
}

/// The path of a member's result file, printed in the event.
pub(crate) fn result_path(team_dir: &Path, role: &Role) -> String {
    team_dir
        .join("outbox")
        .join(format!("{role}.md"))
        .display()
        .to_string()
}

/// One event with its `[teamwire:<team>]` prefix, cut to `EVENT_MAX`.
pub(crate) fn event(team: &str, text: &str) -> String {
    one_line(&format!("[teamwire:{team}] {text}"), EVENT_MAX).into_string()
}
