//! What validation needs. A new team has no database yet, so validation
//! cannot take a `Ctx` (whose store is open, and opening creates files).

use std::path::{Path, PathBuf};

use teamwire_core::{Clock, Config, HarnessId, HostId, Platform, Registry};

/// The adapters and facts `validate` reads. It holds no store: when the
/// team's database exists, `validate` opens it read-only itself, and when it
/// does not, nothing is created (LCH-2, INV-3).
pub struct ValidateEnv<'a> {
    pub home: &'a Path,
    pub registry: &'a Registry,
    pub platform: &'a dyn Platform,
    /// Time for the one check that waits on a host (the lead's locator).
    pub clock: &'a dyn Clock,
    pub config: &'a Config,
    /// The lead's harness: the one wiring picked (`claude-code` or `pi`).
    /// It is also the default harness of a member.
    pub lead_harness: HarnessId,
    /// The host detected for the lead. It is the default host of a member.
    pub lead_host: HostId,
    /// The user's home directory: a member may not start there.
    pub user_home: Option<PathBuf>,
    /// The `teamwire` binary the members' hooks call.
    pub teamwire_bin: PathBuf,
}

impl std::fmt::Debug for ValidateEnv<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateEnv")
            .field("home", &self.home)
            .field("lead_harness", &self.lead_harness)
            .field("lead_host", &self.lead_host)
            .finish_non_exhaustive()
    }
}
