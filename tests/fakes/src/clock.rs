//! `FakeClock`: time moves only when a test says so.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use teamwire_core::{Clock, Timestamp};

use crate::calls::lock;

#[derive(Debug)]
struct State {
    wall: Timestamp,
    offset: Duration,
}

/// A `Clock` whose wall time and monotonic time stand still until
/// `advance`, `sleep_and_wake` or `set_wall` is called. Clones share one
/// timeline, so a service and its test see the same time.
#[derive(Debug, Clone)]
pub struct FakeClock {
    base: Instant,
    state: Arc<Mutex<State>>,
}

impl FakeClock {
    /// 2026-09-29T12:00:00.000Z, an arbitrary fixed start.
    pub const DEFAULT_START_MILLIS: i64 = 1_790_596_800_000;

    pub fn new() -> Self {
        let start = Timestamp::from_unix_millis(Self::DEFAULT_START_MILLIS)
            .unwrap_or_else(|| Timestamp::from_datetime(time::OffsetDateTime::UNIX_EPOCH));
        Self::starting_at(start)
    }

    pub fn starting_at(wall: Timestamp) -> Self {
        #[expect(
            clippy::disallowed_methods,
            reason = "FakeClock takes the base Instant once; every later reading is base plus the advanced offset"
        )]
        let base = Instant::now();
        Self {
            base,
            state: Arc::new(Mutex::new(State {
                wall,
                offset: Duration::ZERO,
            })),
        }
    }

    /// Moves wall time and monotonic time forward by the same amount.
    pub fn advance(&self, by: Duration) {
        let mut state = lock(&self.state);
        state.offset += by;
        state.wall = state.wall.checked_add(by).unwrap_or(state.wall);
    }

    /// A laptop that slept for `slept` and woke: the wall clock jumps and
    /// the monotonic clock does not (macOS stops it during sleep).
    pub fn sleep_and_wake(&self, slept: Duration) {
        let mut state = lock(&self.state);
        state.wall = state.wall.checked_add(slept).unwrap_or(state.wall);
    }

    /// Sets the wall clock to any value, backwards too (an NTP step).
    /// The monotonic clock is untouched.
    pub fn set_wall(&self, wall: Timestamp) {
        lock(&self.state).wall = wall;
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        lock(&self.state).wall
    }

    fn monotonic(&self) -> Instant {
        self.base + lock(&self.state).offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_stands_still_until_advanced() {
        let clock = FakeClock::new();
        let (w0, m0) = (clock.now(), clock.monotonic());
        assert_eq!((clock.now(), clock.monotonic()), (w0, m0));
        clock.advance(Duration::from_secs(90));
        assert_eq!(clock.now().saturating_since(w0), Duration::from_secs(90));
        assert_eq!(clock.monotonic() - m0, Duration::from_secs(90));
    }

    #[test]
    fn sleep_and_wake_moves_only_the_wall_clock() {
        let clock = FakeClock::new();
        let (w0, m0) = (clock.now(), clock.monotonic());
        clock.sleep_and_wake(Duration::from_secs(3600));
        assert_eq!(clock.now().saturating_since(w0), Duration::from_secs(3600));
        assert_eq!(clock.monotonic(), m0);
    }

    #[test]
    fn wall_clock_can_step_backwards_and_clones_share_time() {
        let clock = FakeClock::new();
        let twin = clock.clone();
        let w0 = clock.now();
        let earlier = Timestamp::from_unix_millis(FakeClock::DEFAULT_START_MILLIS - 5000).unwrap();
        twin.set_wall(earlier);
        assert_eq!(clock.now(), earlier);
        assert!(clock.now() < w0);
    }
}
