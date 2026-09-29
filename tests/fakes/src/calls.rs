//! The call log every in-process fake keeps, so a test can assert what a
//! service did to the outside world and whether each call could change it.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Whether a call could change anything outside the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallKind {
    Read,
    Write,
}

/// One recorded call: the trait method, its kind, and a short description
/// of the arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub name: &'static str,
    pub kind: CallKind,
    pub detail: String,
}

/// An append-only list of calls behind a lock, shared by all clones of a fake.
#[derive(Debug, Default)]
pub struct CallLog {
    calls: Mutex<Vec<Call>>,
}

impl CallLog {
    pub fn record(&self, name: &'static str, kind: CallKind, detail: impl Into<String>) {
        lock(&self.calls).push(Call {
            name,
            kind,
            detail: detail.into(),
        });
    }

    pub fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    pub fn count(&self, kind: CallKind) -> usize {
        lock(&self.calls).iter().filter(|c| c.kind == kind).count()
    }

    pub fn count_named(&self, name: &str) -> usize {
        lock(&self.calls).iter().filter(|c| c.name == name).count()
    }

    pub fn names(&self) -> Vec<&'static str> {
        lock(&self.calls).iter().map(|c| c.name).collect()
    }

    pub fn clear(&self) {
        lock(&self.calls).clear();
    }
}

/// A fake never panics because another test thread panicked while holding
/// its lock: the data is plain and stays usable.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_counts_by_kind_and_name() {
        let log = CallLog::default();
        log.record("state", CallKind::Read, "p1");
        log.record("submit", CallKind::Write, "p1");
        log.record("state", CallKind::Read, "p2");
        assert_eq!(log.count(CallKind::Read), 2);
        assert_eq!(log.count(CallKind::Write), 1);
        assert_eq!(log.count_named("state"), 2);
        assert_eq!(log.names(), ["state", "submit", "state"]);
        log.clear();
        assert!(log.calls().is_empty());
    }
}
