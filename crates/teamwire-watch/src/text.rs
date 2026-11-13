//! The announcement keys and the wording of each event (`docs/api/cli.md`
//! "`teamwire watch`"). A key names one occurrence, so it carries the
//! generation or episode number that makes the next occurrence a new key.

use std::time::Duration;

use teamwire_core::{Role, Timestamp, Ulid, one_line};

pub(crate) use teamwire_deliver::{agent_text, result_path, result_written};

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

/// The poll's time as its prefix carries it: `17:24:07Z`. A `Timestamp` is
/// UTC by construction, so its hour needs no conversion.
fn printed_at(at: Timestamp) -> String {
    let at = at.as_datetime();
    format!("{:02}:{:02}:{:02}Z", at.hour(), at.minute(), at.second())
}

/// What an event that points the lead at a file ends with. `COMPLETED`,
/// `UNDELIVERED` and `UNCONFIRMED` all end this way, and the path is the
/// part the lead opens, so it is not the part a long event loses.
const RESULT_MARK: &str = " Result: ";

/// One event with its `[teamwire:<team> HH:MM:SSZ]` prefix, cut to
/// `EVENT_MAX`. The time is the poll's own, so a lead can tell a line
/// printed before its last `launch --add` or reprompt from a current one
/// (WAT-21).
///
/// An event that ends `Result: <path>` keeps the whole path: the summary or
/// reason before it gives way first, because a cut path names no file the
/// lead can open. Only a path that alone fills the cap is cut.
pub(crate) fn event(team: &str, at: Timestamp, text: &str) -> String {
    let head = format!("[teamwire:{team} {}] ", printed_at(at));
    let Some((said, path)) = text.rsplit_once(RESULT_MARK) else {
        return one_line(&format!("{head}{text}"), EVENT_MAX).into_string();
    };
    // The mark is rebuilt around the sanitised path, because `one_line`
    // trims the space a leading mark would start with.
    let tail = format!("{RESULT_MARK}{}", one_line(path, EVENT_MAX));
    let room = EVENT_MAX.saturating_sub(head.chars().count() + tail.chars().count());
    let said = one_line(said, room);
    one_line(&format!("{head}{said}{tail}"), EVENT_MAX).into_string()
}

/// A `blocked` result with this summary is a question round (WAT-22): after
/// an optional `handoff: `, it matches `^[1-9][0-9]* questions? for the
/// parent\.`. The member waits for the lead's answers, so it is not done.
pub(crate) fn asks_the_parent(summary: &str) -> bool {
    let s = summary.strip_prefix("handoff: ").unwrap_or(summary);
    let rest = s.trim_start_matches(|c: char| c.is_ascii_digit());
    rest.len() < s.len()
        && !s.starts_with('0')
        && [" questions for the parent.", " question for the parent."]
            .iter()
            .any(|tail| rest.starts_with(tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-29T12:00:00Z, the fake clock's start.
    fn at() -> Timestamp {
        Timestamp::from_unix_millis(1_790_596_800_000).expect("a valid time")
    }

    /// WAT-1: the path is what the lead opens, so a summary at its cap
    /// gives way to it instead of pushing it out of the event.
    #[test]
    fn a_long_summary_gives_way_to_the_result_path() {
        let path = "/Users/someone/.teamwire/teams/alpha/outbox/api.md";
        let text = format!("COMPLETED api (done) {} Result: {path}", "y".repeat(200));
        let got = event("alpha", at(), &text);
        assert_eq!(got.chars().count(), EVENT_MAX);
        assert!(got.ends_with(&format!(" Result: {path}")), "{got}");
        assert!(
            got.starts_with("[teamwire:alpha 12:00:00Z] COMPLETED api (done) yyy"),
            "{got}"
        );
        assert!(got.contains('…'), "the summary was cut: {got}");
    }

    /// A reason and a path that fit are left alone, mark and spacing too.
    #[test]
    fn an_event_that_fits_is_unchanged() {
        let got = event("alpha", at(), "UNDELIVERED api no_host Result: /t/api.md");
        assert_eq!(
            got,
            "[teamwire:alpha 12:00:00Z] UNDELIVERED api no_host Result: /t/api.md"
        );
    }

    /// An event that names no file is cut at its end, as before.
    #[test]
    fn an_event_without_a_result_path_is_cut_at_its_end() {
        let got = event("alpha", at(), &format!("SILENT {}", "x".repeat(400)));
        assert_eq!(got.chars().count(), EVENT_MAX);
        assert!(got.ends_with('…'), "{got}");
    }

    /// A path that alone fills the cap leaves the summary nothing, and the
    /// cap still holds: nothing a poll prints is longer than `EVENT_MAX`.
    #[test]
    fn a_path_longer_than_the_cap_is_still_cut() {
        let path = "/x".repeat(200);
        let text = format!("COMPLETED api (done) summary Result: {path}");
        let got = event("alpha", at(), &text);
        assert_eq!(got.chars().count(), EVENT_MAX);
        assert!(
            got.starts_with("[teamwire:alpha 12:00:00Z] Result: /x"),
            "{got}"
        );
    }
}
