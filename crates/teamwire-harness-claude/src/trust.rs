//! The folder-trust step. When a dialog is on screen, is it the plain one
//! that teamwire may answer?
//!
//! Never answers a dialog that also grants folder-declared commands
//! (INV-25), and never writes `~/.claude.json`. Launch refuses a home
//! directory before it gets here.

use std::time::Duration;

use teamwire_core::screen::{STARTUP_HOST_TIMEOUT, StartupPoll, has_any, squash};
use teamwire_core::{
    HarnessError, HarnessErrorCode, Host, Key, Place, ScreenState, StartupOutcome, TrustPolicy,
};

use crate::screen::{ESCALATION_MARKERS, PLAIN_TRUST_MARKERS, classify, last_pointer_text};

/// The option that trusts the folder.
const ACCEPT_OPTION: &str = "Yes, I trust this folder";

/// The timeout of each key teamwire sends to a starting member.
const HOST_TIMEOUT: Duration = STARTUP_HOST_TIMEOUT;

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
    let mut left = poll.steps;
    while left > 0 {
        left -= 1;
        let Some(raw) = poll.read(host, place, &mut left)? else {
            std::thread::sleep(poll.step);
            continue;
        };
        // The stronger dialog reuses the plain dialog's title and its "Yes, I
        // trust this folder" button, so escalation is checked first and
        // refused whatever the policy says.
        if escalated(&raw) {
            return Ok(StartupOutcome::RefusedEscalated);
        }
        if has_any(&squash(&raw), &PLAIN_TRUST_MARKERS) {
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

/// True when the screen shows an escalation line anywhere. Every read checks
/// it, because a capture taken while the dialog is still being drawn can show
/// the title before the escalation lines (INV-25).
fn escalated(screen: &str) -> bool {
    has_any(&squash(screen), &ESCALATION_MARKERS)
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
    let mut left = poll.steps;
    while left > 0 {
        left -= 1;
        let Some(screen) = poll.read(host, place, &mut left)? else {
            std::thread::sleep(poll.step);
            continue;
        };
        if escalated(&screen) {
            return Ok(StartupOutcome::RefusedEscalated);
        }
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
    let mut left = poll.steps;
    while left > 0 {
        left -= 1;
        if let Some(screen) = poll.read(host, place, &mut left)? {
            // Enter is already sent here, so an escalated dialog seen now was
            // accepted and Claude Code has saved trust for the folder. That is
            // not `RefusedEscalated`, whose message asks the lead to answer a
            // dialog that will not show again.
            if escalated(&screen) {
                return Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    "the trust dialog showed the folder's escalation lines only after teamwire \
                     accepted it, so Claude Code now trusts this folder and its own settings apply \
                     without asking; review the folder's .claude/ and .mcp.json before using it",
                ));
            }
            if classify(&after_dialog(&screen)) == ScreenState::InputPrompt {
                return Ok(StartupOutcome::AnsweredPlainTrust);
            }
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "the input prompt did not appear after the trust answer",
    ))
}
