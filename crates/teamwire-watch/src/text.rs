//! The announcement keys and the wording of each event (`docs/api/cli.md`
//! "`teamwire watch`"). A key names one occurrence, so it carries the
//! generation or episode number that makes the next occurrence a new key.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Role, Timestamp, Ulid, one_line};

pub(crate) use teamwire_deliver::agent_text;

/// The longest one event may be. A poll joins several, so each is cut alone
/// and a long summary cannot push a later event out of the line.
const EVENT_MAX: usize = 300;
/// The longest a whole line may be. A poll whose events do not all fit
/// prints those that do and leaves the rest for a later poll.
pub(crate) const LINE_MAX: usize = 2000;
/// What a poll puts between two events in its line.
pub(crate) const SEP: &str = " | ";

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

pub(crate) fn unreported_key(role: &Role, generation: u32) -> String {
    format!("UNREPORTED:{role}:{generation}")
}

pub(crate) fn context_key(role: &Role, generation: u32) -> String {
    format!("CONTEXT:{role}:{generation}")
}

pub(crate) fn result_key(kind: &str, completion: &Ulid) -> String {
    format!("{kind}:{completion}")
}

pub(crate) fn question_key(kind: &str, message: &Ulid) -> String {
    format!("{kind}:{message}")
}

pub(crate) fn unread_key(message: &Ulid, to: &Role) -> String {
    format!("UNREAD:{message}:{to}")
}

pub(crate) fn unanswered_key(message: &Ulid) -> String {
    format!("UNANSWERED:{message}")
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

/// True when the member's result file is there and, if the completion
/// recorded a hash, still holds that text.
pub(crate) fn result_written(team_dir: &Path, role: &Role, sha256: Option<&[u8; 32]>) -> bool {
    let outbox = team_dir.join("outbox").join(format!("{role}.md"));
    std::fs::read(&outbox)
        .is_ok_and(|bytes| sha256.is_none_or(|h| teamwire_store::sha256(&bytes) == *h))
}

/// The file the lead should open for a member's result, printed in the
/// event: its result file, unless that is missing or no longer holds the
/// text whose hash the completion recorded. Then its report, which the
/// member wrote itself, is the better pointer.
pub(crate) fn result_path(team_dir: &Path, role: &Role, sha256: Option<&[u8; 32]>) -> String {
    let dir = if result_written(team_dir, role, sha256) {
        "outbox"
    } else {
        "reports"
    };
    team_dir
        .join(dir)
        .join(format!("{role}.md"))
        .display()
        .to_string()
}

/// The poll's time as its prefix carries it: `17:24:07Z`. A `Timestamp` is
/// UTC by construction, so its hour needs no conversion.
fn printed_at(at: Timestamp) -> String {
    let at = at.as_datetime();
    format!("{:02}:{:02}:{:02}Z", at.hour(), at.minute(), at.second())
}

/// One event with its `[teamwire:<team> HH:MM:SSZ]` prefix, cut to
/// `EVENT_MAX`. The time is the poll's own, so a lead can tell a line
/// printed before its last `launch --add` or reprompt from a current one
/// (WAT-21).
pub(crate) fn event(team: &str, at: Timestamp, text: &str) -> String {
    one_line(
        &format!("[teamwire:{team} {}] {text}", printed_at(at)),
        EVENT_MAX,
    )
    .into_string()
}
