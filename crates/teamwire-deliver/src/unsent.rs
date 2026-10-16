//! After a submit on a host that does not say whether the text was taken
//! (tmux: paste, then Enter), look at the screen once. A line that still sits
//! in the input box means the Enter came before the session took the paste.
//! The harness reads the screen for that; this file holds the timing.

use std::thread;
use std::time::Duration;

use teamwire_core::{Harness, Host, Key, OneLine, Place, ScreenState};

use crate::budget::Budget;

/// How long the session gets to clear its box after the Enter.
const SETTLE: Duration = Duration::from_millis(300);
/// How much of the screen is read.
const TAIL_LINES: u16 = 40;

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
    if harness.screen_state(&tail) == ScreenState::Dialog || !harness.line_pending(&tail, line) {
        return;
    }
    let Ok(timeout) = budget.call() else { return };
    if host.send_keys(place, &[Key::Enter], timeout).is_ok() {
        tracing::info!(place = %place.id, "the typed line was still in the input box; pressed Enter once more");
    }
}
