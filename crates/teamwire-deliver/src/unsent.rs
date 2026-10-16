//! After a submit on a host that does not say whether the text was taken
//! (tmux: paste, then Enter), look at the screen once. A line that still sits
//! in the input box means the Enter came before the session took the paste.

use std::thread;
use std::time::Duration;

use teamwire_core::{Harness, Host, Key, OneLine, Place, ScreenState};

use crate::budget::Budget;

/// How long the session gets to clear its box after the Enter.
const SETTLE: Duration = Duration::from_millis(300);
/// How much of the screen is read.
const TAIL_LINES: u16 = 40;
/// How much of the typed line is looked for. The box wraps a long line, but
/// its first row holds the start.
const PREFIX_CHARS: usize = 40;

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text after the last pointer line (`❯` or `>`) of the screen. That line
/// is the input box: a line Claude echoed into its transcript is always
/// followed by the box's own pointer line.
fn input_box_text(tail: &str) -> Option<String> {
    tail.lines().rev().find_map(|line| {
        let line = line.trim_matches(|c: char| c.is_whitespace() || "│┃|".contains(c));
        line.strip_prefix('❯')
            .or_else(|| line.strip_prefix('>'))
            .map(squash)
    })
}

fn is_still_typed(tail: &str, line: &OneLine) -> bool {
    let wanted: String = squash(line.as_str()).chars().take(PREFIX_CHARS).collect();
    input_box_text(tail).is_some_and(|text| text.contains(&wanted))
}

/// Presses Enter once more when the typed line is still in the input box and
/// the screen is not a dialog. Never types the text again (INV-5). Any
/// failure here leaves the delivery as it was: the host already accepted the
/// text.
pub(crate) fn enter_again_if_unsent(
    harness: &dyn Harness,
    host: &dyn Host,
    place: &Place,
    line: &OneLine,
    budget: &Budget<'_>,
) {
    thread::sleep(budget.left().map_or(SETTLE, |left| SETTLE.min(left)));
    let Ok(timeout) = budget.call() else { return };
    let Ok(tail) = host.read_tail(place, TAIL_LINES, timeout) else {
        return;
    };
    if harness.screen_state(&tail) == ScreenState::Dialog || !is_still_typed(&tail, line) {
        return;
    }
    let Ok(timeout) = budget.call() else { return };
    if host.send_keys(place, &[Key::Enter], timeout).is_ok() {
        tracing::info!(place = %place.id, "the typed line was still in the input box; pressed Enter once more");
    }
}
