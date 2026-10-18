//! The folder-trust step. When a dialog is on screen, is it the plain one
//! that teamwire may answer?
//!
//! Never answers a dialog that also grants folder-declared commands
//! (INV-25), and never writes `~/.claude.json`. Launch refuses a home
//! directory before it gets here.

use std::time::Duration;

use teamwire_core::{
    HarnessError, HarnessErrorCode, Host, HostErrorCode, Key, Place, ScreenState, StartupOutcome,
    TrustPolicy,
};

use crate::screen::{
    ESCALATION_MARKERS, PLAIN_TRUST_MARKERS, classify, has_any, last_pointer_text, squash,
};

/// The option that trusts the folder.
const ACCEPT_OPTION: &str = "Yes, I trust this folder";

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

/// The screen, or `None` for "no screen this time": a read that ran out of
/// its time (a slow start) or that herdr refused because the agent is working.
/// The loops read again on their next step. Any other host error ends the step.
fn read(host: &dyn Host, place: &Place) -> Result<Option<String>, HarnessError> {
    match host.read_tail(place, TAIL_LINES, HOST_TIMEOUT) {
        Ok(screen) => Ok(Some(screen)),
        Err(e) if matches!(e.code, HostErrorCode::Timeout | HostErrorCode::Busy) => Ok(None),
        Err(e) => Err(e.into()),
    }
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
        let Some(raw) = read(host, place)? else {
            std::thread::sleep(poll.step);
            continue;
        };
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
    // this folder". Never `1`: it does not pick the first line. A slow start
    // can drop or delay a key, so Enter waits until the screen shows the
    // pointer on the accepting option. A second Down is sent once, after a
    // long wait: a late first Down would otherwise be undone by it.
    let patience = (poll.steps / 3).max(1);
    let mut downs = 0;
    let mut since_down = 0;
    for _ in 0..poll.steps {
        let Some(screen) = read(host, place)? else {
            std::thread::sleep(poll.step);
            continue;
        };
        if classify(&after_dialog(&screen)) == ScreenState::InputPrompt {
            return Ok(StartupOutcome::NoDialog);
        }
        match last_pointer_text(&screen) {
            Some(text) if text.contains(ACCEPT_OPTION) => return confirm(host, place, poll),
            Some(_) if downs == 0 || (downs == 1 && since_down >= patience) => {
                host.send_keys(place, &[Key::Down], HOST_TIMEOUT)?;
                downs += 1;
                since_down = 0;
            }
            _ => since_down += 1,
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "the trust dialog never showed the accepting option selected",
    ))
}

/// Presses Enter on the selected "Yes, I trust this folder" and waits for the
/// input prompt.
fn confirm(
    host: &dyn Host,
    place: &Place,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    host.send_keys(place, &[Key::Enter], HOST_TIMEOUT)?;
    for _ in 0..poll.steps {
        if let Some(screen) = read(host, place)?
            && classify(&after_dialog(&screen)) == ScreenState::InputPrompt
        {
            return Ok(StartupOutcome::AnsweredPlainTrust);
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "the input prompt did not appear after the trust answer",
    ))
}
