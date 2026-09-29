//! Typing one line into a located session.

use std::thread;
use std::time::Duration;

use teamwire_core::{
    DeliveryError, DeliveryErrorCode, Harness, Host, HostError, HostErrorCode, Liveness, OneLine,
    Place, PlaceState, ScreenState,
};
use teamwire_store::{Ctx, Party};

use crate::DeliveryOutcome;
use crate::locate::{CALL_TIMEOUT, locate_with, recheck};

/// A submit is tried at most this many times (ADR 0003 point 7).
const MAX_TRIES: u32 = 3;
/// No new try starts once this long has passed since the first.
const RETRY_WINDOW: Duration = Duration::from_secs(5);
/// The pause before the next try, times the number of tries so far.
const RETRY_PAUSE: Duration = Duration::from_millis(250);
/// How much of the screen the dialog check reads.
const TAIL_LINES: u16 = 40;

/// Locates `party` and types `line` into its session.
///
/// Nothing is typed unless the host view still confirms the place just
/// before the send and the session is at its input prompt. A host code that
/// proves nothing was submitted is retried, at most 3 tries in about 5
/// seconds. A stall or timeout is `Unconfirmed` and never retried.
pub fn deliver_line(ctx: &Ctx<'_>, party: &Party, line: &OneLine) -> DeliveryOutcome {
    match try_deliver(ctx, party, line) {
        Ok(outcome) => outcome,
        Err(e) => DeliveryOutcome::Undelivered(e),
    }
}

fn try_deliver(
    ctx: &Ctx<'_>,
    party: &Party,
    line: &OneLine,
) -> Result<DeliveryOutcome, DeliveryError> {
    let (harness, host) = ctx.adapters(party)?;
    if !host.capabilities().can_type {
        return Err(DeliveryError::new(
            DeliveryErrorCode::NoHost,
            format!("host {} cannot type into a session", host.id()),
        ));
    }
    if line.as_str().is_empty() {
        // Real herdr refuses empty text, and an empty line carries no message.
        return Err(DeliveryError::new(
            DeliveryErrorCode::HostBadArgument,
            "there is nothing to type",
        ));
    }
    let found = locate_with(ctx, party, harness, host)?;
    recheck(host, &found)?;
    check_input_ready(harness, host, &found.place, &found.session)?;
    Ok(submit_with_retries(ctx, host, &found.place, line))
}

/// ADR 0006 point 3a: never type into a dialog.
fn check_input_ready(
    harness: &dyn Harness,
    host: &dyn Host,
    place: &Place,
    session: &teamwire_core::SessionId,
) -> Result<(), DeliveryError> {
    let blocked = |source: &str| {
        DeliveryError::new(
            DeliveryErrorCode::SessionBlocked,
            format!("{source} reports the session is blocked"),
        )
    };
    if host
        .state(place, CALL_TIMEOUT)
        .map_err(DeliveryError::from)?
        == PlaceState::Blocked
    {
        return Err(blocked("the host"));
    }
    // The harness listing may report the state too. A failed listing is not
    // a reason to refuse: the host view already confirmed the place.
    if harness.is_alive(session, CALL_TIMEOUT) == Ok(Liveness::Alive(PlaceState::Blocked)) {
        return Err(blocked("the harness"));
    }
    if host.capabilities().guards_dialogs {
        return Ok(());
    }
    let tail = host
        .read_tail(place, TAIL_LINES, CALL_TIMEOUT)
        .map_err(DeliveryError::from)?;
    match harness.screen_state(&tail) {
        ScreenState::InputPrompt => Ok(()),
        ScreenState::Dialog | ScreenState::Unknown => Err(DeliveryError::new(
            DeliveryErrorCode::NotAtInputPrompt,
            "the screen does not show the input prompt",
        )),
    }
}

fn submit_with_retries(
    ctx: &Ctx<'_>,
    host: &dyn Host,
    place: &Place,
    line: &OneLine,
) -> DeliveryOutcome {
    let started = ctx.clock.monotonic();
    let mut tries = 0;
    loop {
        tries += 1;
        let err = match host.submit(place, line, CALL_TIMEOUT) {
            Ok(()) => return DeliveryOutcome::HandedOff,
            Err(e) => e,
        };
        match err.code {
            // Only these codes prove the text was not submitted.
            HostErrorCode::NotReady | HostErrorCode::Busy => {
                let in_time =
                    ctx.clock.monotonic().saturating_duration_since(started) < RETRY_WINDOW;
                if tries >= MAX_TRIES || !in_time {
                    return DeliveryOutcome::Undelivered(err.into());
                }
                thread::sleep(RETRY_PAUSE * tries);
            }
            // The text may be on screen: never send it again (INV-5).
            HostErrorCode::Stalled | HostErrorCode::Timeout => {
                return DeliveryOutcome::Unconfirmed(err.into());
            }
            HostErrorCode::Blocked
            | HostErrorCode::NotFound
            | HostErrorCode::BadArgument
            | HostErrorCode::Unsupported
            | HostErrorCode::Other => return DeliveryOutcome::Undelivered(refused(err)),
        }
    }
}

/// A host that refuses to type into a dialog is reported as a blocked
/// session, as the state check would have; every other code keeps its own.
fn refused(err: HostError) -> DeliveryError {
    if err.code == HostErrorCode::Blocked {
        DeliveryError::new(DeliveryErrorCode::SessionBlocked, err.detail)
    } else {
        err.into()
    }
}
