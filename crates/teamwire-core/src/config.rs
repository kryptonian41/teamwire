//! Settings every service reads through `Ctx`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub poll_s: u64,
    pub silent_polls: u32,
    pub startup_grace_s: u64,
    /// The longest a member's setup command may run. Launch kills it at this
    /// limit, and the watch waits this long before it calls a `creating`
    /// member stuck.
    pub setup_timeout_s: u64,
    /// The longest launch waits for the host to start one agent.
    pub spawn_timeout_s: u64,
    pub default_deadline_s: u64,
    pub hook_budget_s: u64,
    pub hook_timeout_s: u64,
    pub delivering_stale_s: u64,
    /// How long close waits, once a member's place is closed, for the
    /// processes that ran in it to end before it removes the worktree.
    pub close_exit_wait_ms: u64,
    pub min_free_bytes: u64,
    pub otel: OtelConfig,
}

impl Default for Config {
    /// The values the design names: a 20 s poll, 3 quiet polls, 90 s grace,
    /// a 600 s setup limit and 150 s spawn limit, a 1800 s deadline, the 40 s hook budget under a 60 s hook timeout,
    /// 90 s before `delivering` counts as interrupted, a 5 s wait for a closed
    /// place's processes to end, and 200 MB free.
    fn default() -> Self {
        Self {
            poll_s: 20,
            silent_polls: 3,
            startup_grace_s: 90,
            setup_timeout_s: 600,
            spawn_timeout_s: 150,
            default_deadline_s: 1800,
            hook_budget_s: 40,
            hook_timeout_s: 60,
            delivering_stale_s: 90,
            close_exit_wait_ms: 5000,
            min_free_bytes: 200 * 1024 * 1024,
            otel: OtelConfig::default(),
        }
    }
}

/// Where OpenTelemetry data goes. No endpoint means export is off.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtelConfig {
    pub endpoint: Option<String>,
    /// Header values often hold an API key, so `Debug` prints only the names (SEC-1).
    pub headers: Vec<(String, String)>,
}

impl fmt::Debug for OtelConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("OtelConfig")
            .field("endpoint", &self.endpoint)
            .field("header_names", &names)
            .finish()
    }
}
