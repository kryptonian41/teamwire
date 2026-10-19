//! A command's store that writes down its first failed write when the
//! command lets go of it, whichever way the command returns.

use std::ops::{Deref, DerefMut};

use teamwire_core::Clock;
use teamwire_store::Store;

/// Owns the store for one command. On drop it calls `Store::record_failure`,
/// so a busy, full, corrupt or io failure leaves its `store.error` row even
/// when the command returned early on that failure.
pub(crate) struct Recorded<'c> {
    store: Store,
    clock: &'c dyn Clock,
    op: &'static str,
}

impl<'c> Recorded<'c> {
    /// `op` is the command's name, the `op` of the row.
    pub(crate) fn new(store: Store, clock: &'c dyn Clock, op: &'static str) -> Self {
        Self { store, clock, op }
    }
}

impl Deref for Recorded<'_> {
    type Target = Store;

    fn deref(&self) -> &Store {
        &self.store
    }
}

impl DerefMut for Recorded<'_> {
    fn deref_mut(&mut self) -> &mut Store {
        &mut self.store
    }
}

impl Drop for Recorded<'_> {
    fn drop(&mut self) {
        self.store.record_failure(self.clock.now(), self.op);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use teamwire_core::{EventType, TeamName};
    use teamwire_fakes::{FakeClock, TestHome};
    use teamwire_store::{Store, StoreError};

    use super::Recorded;

    fn store_errors(store: &Store) -> Vec<serde_json::Value> {
        store
            .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == EventType::StoreError)
            .map(|e| e.data)
            .collect()
    }

    /// A command that gives up on a failed write, the way `?` or an early
    /// `return fail(..)` does.
    fn command(store: Store, clock: &FakeClock) -> Result<(), StoreError> {
        let mut store = Recorded::new(store, clock, "send");
        store.write(|_| Err(StoreError::Busy))?;
        Ok(())
    }

    #[test]
    fn a_command_that_returns_on_a_failed_write_leaves_its_row() {
        let home = TestHome::new().unwrap();
        let team = TeamName::parse("alpha").unwrap();
        let clock = FakeClock::new();
        assert!(command(Store::open(home.path(), &team).unwrap(), &clock).is_err());
        let store = Store::open(home.path(), &team).unwrap();
        assert_eq!(
            store_errors(&store),
            [json!({ "op": "send", "code": "busy" })]
        );
    }
}
