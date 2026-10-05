//! When a watch lease no longer counts. Both `send` (does a watch run?) and
//! a new watch (may it take over?) ask, so the answer lives here.

use std::time::Duration;

use teamwire_core::{Platform, Timestamp};
use teamwire_store::WatchLease;

/// A lease is dead when its process is gone, or when its heartbeat is older
/// than three polls. The second test catches a pid the operating system gave
/// to another process after the watch died: that pid is alive, but nothing
/// has refreshed the heartbeat since.
pub fn watch_lease_is_dead(
    lease: &WatchLease,
    platform: &dyn Platform,
    now: Timestamp,
    poll_s: u64,
) -> bool {
    let silent_for = now.saturating_since(lease.heartbeat_at);
    !platform.pid_alive(lease.pid) || silent_for > Duration::from_secs(poll_s.saturating_mul(3))
}
