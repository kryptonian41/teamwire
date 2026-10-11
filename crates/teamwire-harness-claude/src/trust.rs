//! The folder-trust step. When a dialog is on screen, is it the plain one
//! that teamwire may answer?
//!
//! Never answers a dialog that also grants folder-declared commands
//! (INV-25), and never writes `~/.claude.json`. Launch refuses a home
//! directory before it gets here.

use std::time::Duration;

use teamwire_core::{
    HarnessError, HarnessErrorCode, Host, Key, Place, ScreenState, StartupOutcome, TrustPolicy,
};

use crate::screen::{ESCALATION_MARKERS, PLAIN_TRUST_MARKERS, classify, has_any, squash};

/// How many screen lines to read for the dialog: more than any terminal is
/// tall, so a tall dialog is read whole. Its escalation lines can sit above the
/// last 40 lines, and a marker that is missed lets the dialog be answered
/// (INV-25).
const TAIL_LINES: u16 = 1000;
const HOST_TIMEOUT: Duration = Duration::from_secs(5);

/// How long `answer_startup` waits: `steps` reads, `step` apart. A count of
/// sleeps bounds the wait because the clock is not readable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupPoll {
    pub step: Duration,
    pub steps: u32,
}

impl Default for StartupPoll {
    /// About 30 s in half-second steps.
    fn default() -> Self {
        Self {
            step: Duration::from_millis(500),
            steps: 60,
        }
    }
}

fn read(host: &dyn Host, place: &Place) -> Result<String, HarnessError> {
    Ok(host.read_tail(place, TAIL_LINES, HOST_TIMEOUT)?)
}

/// Watches the member's screen until it shows the input prompt, the plain
/// trust dialog (answered when `policy` allows) or an escalated dialog
/// (never answered). A folder with saved trust shows no dialog, so it ends
/// as `NoDialog`.
pub(crate) fn answer_startup(
    host: &dyn Host,
    place: &Place,
    policy: TrustPolicy,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    for _ in 0..poll.steps {
        let raw = read(host, place)?;
        let tail = squash(&raw);
        // The stronger dialog reuses the plain dialog's title and its "Yes, I
        // trust this folder" button, so escalation is checked first and
        // refused whatever the policy says.
        if has_any(&tail, &ESCALATION_MARKERS) {
            return Ok(StartupOutcome::RefusedEscalated);
        }
        if has_any(&tail, &PLAIN_TRUST_MARKERS) {
            return match policy {
                TrustPolicy::Refuse => Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    "the spec does not accept the folder-trust dialog",
                )),
                TrustPolicy::Accept => accept(host, place, poll),
            };
        }
        if classify(&raw) == ScreenState::InputPrompt {
            return Ok(StartupOutcome::NoDialog);
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "neither the input prompt nor a trust dialog appeared",
    ))
}

/// The screen below the last line of the trust dialog. The dialog's text can
/// stay in the scrollback above the prompt that follows it, and the
/// classifier reads any dialog text near the end as an open dialog, so the
/// confirmation looks only at what the dialog left below it.
fn after_dialog(screen: &str) -> String {
    let lines: Vec<&str> = screen.lines().collect();
    let last = lines.iter().rposition(|l| {
        let l = squash(l);
        has_any(&l, &PLAIN_TRUST_MARKERS) || has_any(&l, &ESCALATION_MARKERS)
    });
    let below = last.map_or(&lines[..], |i| lines.get(i + 1..).unwrap_or_default());
    below.join("\n")
}

fn timeout(what: &str) -> HarnessError {
    HarnessError::new(HarnessErrorCode::Timeout, what)
}

fn accept(
    host: &dyn Host,
    place: &Place,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    // "No, exit" is the preselected line, so Down moves to "Yes, I trust
    // this folder". Never `1`: it does not pick the first line.
    host.send_keys(place, &[Key::Down], HOST_TIMEOUT)?;
    host.send_keys(place, &[Key::Enter], HOST_TIMEOUT)?;
    for _ in 0..poll.steps {
        if classify(&after_dialog(&read(host, place)?)) == ScreenState::InputPrompt {
            return Ok(StartupOutcome::AnsweredPlainTrust);
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "the input prompt did not appear after the trust answer",
    ))
}
