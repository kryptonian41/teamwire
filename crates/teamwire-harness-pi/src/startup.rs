//! The wait for pi's input box after a member starts.
//!
//! A member runs with `--no-approve`, so pi opens no project-trust dialog,
//! and pi 1.4.0 has no other dialog at startup. A dialog seen here is one
//! teamwire did not expect, such as a user extension's prompt, so it is
//! refused and never answered.

use teamwire_core::screen::StartupPoll;
use teamwire_core::{HarnessError, HarnessErrorCode, Host, Place, ScreenState, StartupOutcome};

use crate::screen::{TRUST_MARKER, classify};

pub(crate) fn answer_startup(
    host: &dyn Host,
    place: &Place,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    let mut left = poll.steps;
    while left > 0 {
        left -= 1;
        let Some(screen) = poll.read(host, place, &mut left)? else {
            std::thread::sleep(poll.step);
            continue;
        };
        match classify(&screen) {
            ScreenState::InputPrompt => return Ok(StartupOutcome::NoDialog),
            ScreenState::Dialog => {
                let what = if screen.contains(TRUST_MARKER) {
                    "pi asks to trust the project folder, which --no-approve should prevent"
                } else {
                    "pi shows a dialog at startup"
                };
                return Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    format!(
                        "{what}; open pi once in that folder yourself, then start the team again"
                    ),
                ));
            }
            ScreenState::Unknown => {}
        }
        std::thread::sleep(poll.step);
    }
    Err(HarnessError::new(
        HarnessErrorCode::Timeout,
        "pi's input box did not appear",
    ))
}
