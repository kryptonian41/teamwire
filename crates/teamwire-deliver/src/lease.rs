//! When a watch lease no longer counts. Both `send` (does a watch run?) and
//! a new watch (may it take over?) ask, so the answer lives here.

use std::time::Duration;

use teamwire_core::{Platform, Timestamp};
use teamwire_store::WatchLease;

/// A heartbeat older than this means the watch is gone. The lease row keeps
/// no interval, and a checker cannot guess the holder's `--interval`, so the
/// limit is a constant no sane interval exceeds. A reused pid then blocks a
/// new watch for at most this long and not for ever.
pub const WATCH_LEASE_STALE: Duration = Duration::from_secs(600);

/// A lease is dead when its process is gone, or when its heartbeat is older
/// than `WATCH_LEASE_STALE`. The second test catches a pid the operating
/// system gave to another process after the watch died: that pid is alive,
/// but nothing has refreshed the heartbeat since.
pub fn watch_lease_is_dead(lease: &WatchLease, platform: &dyn Platform, now: Timestamp) -> bool {
    !platform.pid_alive(lease.pid) || now.saturating_since(lease.heartbeat_at) > WATCH_LEASE_STALE
}
