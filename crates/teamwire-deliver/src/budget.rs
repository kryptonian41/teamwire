//! The time a delivery may still use (ADR 0007 point 6a).

use std::time::{Duration, Instant};

use teamwire_core::{Clock, DeliveryError, DeliveryErrorCode};

/// Every host and harness call in a delivery gets at most this long (DEL-14).
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// An optional deadline, measured with the context's clock.
pub(crate) struct Budget<'a> {
    clock: &'a dyn Clock,
    deadline: Option<Instant>,
}

impl<'a> Budget<'a> {
    pub(crate) fn new(clock: &'a dyn Clock, deadline: Option<Instant>) -> Self {
        Self { clock, deadline }
    }

    /// The timeout for the next call: its own bound or the time left,
    /// whichever is smaller. A call that cannot start before the deadline is
    /// `budget_exhausted`, and nothing has been sent by it.
    pub(crate) fn call(&self) -> Result<Duration, DeliveryError> {
        let Some(deadline) = self.deadline else {
            return Ok(CALL_TIMEOUT);
        };
        let left = deadline.saturating_duration_since(self.clock.monotonic());
        if left.is_zero() {
            return Err(DeliveryError::new(
                DeliveryErrorCode::BudgetExhausted,
                "the delivery budget ran out before this call",
            ));
        }
        Ok(left.min(CALL_TIMEOUT))
    }

    /// Time left for a pause between tries, or `None` with no deadline.
    pub(crate) fn left(&self) -> Option<Duration> {
        self.deadline
            .map(|d| d.saturating_duration_since(self.clock.monotonic()))
    }
}
