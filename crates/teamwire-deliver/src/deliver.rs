//! Typing one line into a located session.

use std::thread;
use std::time::{Duration, Instant};

use teamwire_core::{
    DeliveryError, DeliveryErrorCode, Harness, Host, HostError, HostErrorCode, Liveness, OneLine,
    Place, PlaceState, ScreenState, SubmitAck,
};
use teamwire_store::{Ctx, Party};

use crate::DeliveryOutcome;
use crate::budget::Budget;
use crate::locate::{Located, locate_with, recheck};
use crate::unsent::enter_again_if_unsent;

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
pub fn deliver_line(ctx: &mut Ctx<'_>, party: &Party, line: &OneLine) -> DeliveryOutcome {
    run(ctx, party, line, None)
}

/// Like `deliver_line`, for a caller with a time limit (the turn-end hook's
/// 40 s, ADR 0007 point 6a). Every harness and host call gets the smaller of
/// its own bound and the time left before `deadline`, measured with
/// `ctx.clock.monotonic()`. A call that cannot start before the deadline
/// gives `Undelivered` with `budget_exhausted`: nothing was typed by then. A
/// submit cut short by the deadline is `Unconfirmed`, as any timeout is.
pub fn deliver_line_within(
    ctx: &mut Ctx<'_>,
    party: &Party,
    line: &OneLine,
    deadline: Instant,
) -> DeliveryOutcome {
    run(ctx, party, line, Some(deadline))
}

fn run(
    ctx: &mut Ctx<'_>,
    party: &Party,
    line: &OneLine,
    deadline: Option<Instant>,
) -> DeliveryOutcome {
    let budget = Budget::new(ctx.clock, deadline);
    let (outcome, reentered_at) = match try_deliver(ctx, party, line, &budget) {
        Ok(done) => done,
        Err(e) => (DeliveryOutcome::Undelivered(e), None),
    };
    // After every host call, so no write transaction is open during one. The
    // line is already handed off, so a failed write changes nothing. A locked
    // store would make the write wait past the caller's deadline, so it is
    // skipped when no time is left.
    if let Some(place) = reentered_at {
        if budget.call().is_err() {
            tracing::warn!("the delivery budget is spent; the second Enter is not recorded");
            return outcome;
        }
        let at = ctx.clock.now();
        if let Err(e) = ctx
            .store
            .write(|tx| tx.record_delivery_reentered(at, party, &place))
        {
            tracing::warn!(error = %e, "could not record the second Enter");
        }
    }
    outcome
}

fn try_deliver(
    ctx: &Ctx<'_>,
    party: &Party,
    line: &OneLine,
    budget: &Budget<'_>,
) -> Result<(DeliveryOutcome, Option<String>), DeliveryError> {
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
    let found = locate_with(ctx, party, harness, host, budget)?;
    // The dialog and readiness checks make several adapter calls, so they run
    // first. The occupant recheck comes last, with only the submit after it
    // (ADR 0006 point 3); a retry repeats it.
    check_input_ready(harness, host, &found.place, &found.session, budget)?;
    submit_with_retries(ctx, harness, host, &found, line, budget)
}

/// ADR 0006 point 3a: never type into a dialog.
fn check_input_ready(
    harness: &dyn Harness,
    host: &dyn Host,
    place: &Place,
    session: &teamwire_core::SessionId,
    budget: &Budget<'_>,
) -> Result<(), DeliveryError> {
    let blocked = |source: &str| {
        DeliveryError::new(
            DeliveryErrorCode::SessionBlocked,
            format!("{source} reports the session is blocked"),
        )
    };
    if host
        .state(place, budget.call()?)
        .map_err(DeliveryError::from)?
        == PlaceState::Blocked
    {
        return Err(blocked("the host"));
    }
    // The harness listing may report the state too. A failed listing is not
    // a reason to refuse: the host view already confirmed the place.
    if harness.is_alive(session, budget.call()?) == Ok(Liveness::Alive(PlaceState::Blocked)) {
        return Err(blocked("the harness"));
    }
    if host.capabilities().guards_dialogs {
        return check_box_empty(harness, host, place, budget);
    }
    // A screen the harness cannot read yet (a redraw, a slow start) is read
    // again a few times, as a busy host is. A dialog is final at once.
    for tries in 1..=MAX_TRIES {
        let timeout = budget.call()?;
        // The harness tells dim placeholder text from typed text by its
        // style codes.
        let tail = host
            .read_tail_styled(place, TAIL_LINES, timeout)
            .map_err(DeliveryError::from)?;
        match harness.screen_state(&tail) {
            ScreenState::InputPrompt => return Ok(()),
            ScreenState::Unknown if tries < MAX_TRIES => {
                let pause = RETRY_PAUSE * tries;
                thread::sleep(budget.left().map_or(pause, |left| pause.min(left)));
            }
            ScreenState::Dialog | ScreenState::Unknown => break,
        }
    }
    Err(DeliveryError::new(
        DeliveryErrorCode::NotAtInputPrompt,
        "the screen does not show the input prompt",
    ))
}

/// A host that guards dialogs refuses them itself, but still types after
/// text a person left in the input box. The box is read again a few times,
/// as an unreadable screen is, because the person may be about to send it.
fn check_box_empty(
    harness: &dyn Harness,
    host: &dyn Host,
    place: &Place,
    budget: &Budget<'_>,
) -> Result<(), DeliveryError> {
    for tries in 1..=MAX_TRIES {
        // A screen the host cannot give says nothing about the box, and the
        // host still guards dialogs, so the line goes as it did before.
        let Ok(tail) = host.read_tail_styled(place, TAIL_LINES, budget.call()?) else {
            return Ok(());
        };
        if !harness.box_holds_text(&tail) {
            return Ok(());
        }
        if tries < MAX_TRIES {
            let pause = RETRY_PAUSE * tries;
            thread::sleep(budget.left().map_or(pause, |left| pause.min(left)));
        }
    }
    Err(DeliveryError::new(
        DeliveryErrorCode::NotAtInputPrompt,
        "the input box holds text someone typed",
    ))
}

fn submit_with_retries(
    ctx: &Ctx<'_>,
    harness: &dyn Harness,
    host: &dyn Host,
    found: &Located,
    line: &OneLine,
    budget: &Budget<'_>,
) -> Result<(DeliveryOutcome, Option<String>), DeliveryError> {
    let place = &found.place;
    let started = ctx.clock.monotonic();
    let mut tries = 0;
    loop {
        tries += 1;
        // A failed recheck proves nothing was typed, so it is a refusal
        // with its own reason, also between retries.
        recheck(host, found, budget)?;
        let timeout = match budget.call() {
            Ok(t) => t,
            Err(e) => return Ok((DeliveryOutcome::Undelivered(e), None)),
        };
        let err = match host.submit(place, line, timeout) {
            Ok(()) => {
                // ADR 0003 point 5: with no acknowledgement, `Ok` proves
                // only that the keys were accepted.
                let reentered = host.capabilities().submit_ack == SubmitAck::None
                    && enter_again_if_unsent(harness, host, place, line, budget);
                return Ok((
                    DeliveryOutcome::HandedOff,
                    reentered.then(|| place.id.clone()),
                ));
            }
            Err(e) => e,
        };
        match err.code {
            // Only these codes prove the text was not submitted.
            HostErrorCode::NotReady | HostErrorCode::Busy => {
                let in_time =
                    ctx.clock.monotonic().saturating_duration_since(started) < RETRY_WINDOW;
                if tries >= MAX_TRIES || !in_time {
                    return Ok((DeliveryOutcome::Undelivered(err.into()), None));
                }
                let pause = RETRY_PAUSE * tries;
                thread::sleep(budget.left().map_or(pause, |left| pause.min(left)));
            }
            // The text may be on screen: never send it again (INV-5).
            HostErrorCode::Stalled | HostErrorCode::Timeout => {
                return Ok((DeliveryOutcome::Unconfirmed(err.into()), None));
            }
            HostErrorCode::Blocked
            | HostErrorCode::NotFound
            | HostErrorCode::BadArgument
            | HostErrorCode::Unsupported
            | HostErrorCode::Other => {
                return Ok((DeliveryOutcome::Undelivered(refused(err)), None));
            }
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
