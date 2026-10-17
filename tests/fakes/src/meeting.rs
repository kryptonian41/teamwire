//! `Meeting`: makes a fake's calls wait for each other, so a test can prove
//! that code runs them at the same time without measuring wall time.

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use crate::calls::lock;

#[derive(Debug, Default)]
struct Count {
    /// How many calls must be in progress before any goes on. 0 is off.
    want: usize,
    wait: Duration,
    in_progress: usize,
    peak: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Meeting {
    count: Mutex<Count>,
    changed: Condvar,
}

impl Meeting {
    pub(crate) fn set(&self, want: usize, wait: Duration) {
        let mut count = lock(&self.count);
        count.want = want;
        count.wait = wait;
        count.peak = 0;
    }

    pub(crate) fn peak(&self) -> usize {
        lock(&self.count).peak
    }

    /// Waits until `want` calls have been in progress at once, or `wait` has
    /// passed, then lets the call go on. The wait reads the peak, which never
    /// drops, so a call that ends first cannot strand the others. The guard
    /// counts the call out when it ends.
    pub(crate) fn join(&self) -> Left<'_> {
        let mut count = lock(&self.count);
        count.in_progress += 1;
        count.peak = count.peak.max(count.in_progress);
        self.changed.notify_all();
        let wait = count.wait;
        drop(
            self.changed
                .wait_timeout_while(count, wait, |c| c.want > 0 && c.peak < c.want)
                .unwrap_or_else(PoisonError::into_inner),
        );
        Left(self)
    }
}

/// Counts the call out when it ends.
pub(crate) struct Left<'a>(&'a Meeting);

impl Drop for Left<'_> {
    fn drop(&mut self) {
        lock(&self.0.count).in_progress -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_at_the_same_time_meet_and_calls_one_by_one_do_not() {
        let meeting = Meeting::default();
        meeting.set(3, Duration::from_secs(5));
        std::thread::scope(|s| {
            for _ in 0..3 {
                s.spawn(|| drop(meeting.join()));
            }
        });
        assert_eq!(meeting.peak(), 3);

        meeting.set(2, Duration::from_millis(20));
        drop(meeting.join());
        drop(meeting.join());
        assert_eq!(meeting.peak(), 1);
    }
}
