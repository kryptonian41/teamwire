//! What one watch process remembers between polls. Nothing here has to
//! survive a restart: everything that must is an announcement in the store.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use teamwire_core::{Role, Timestamp};

/// Why the watch stopped polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEnd {
    /// `TEAM-COMPLETE` was printed.
    TeamComplete,
    /// The team is closed, or has no row. `TEAM-CLOSED` was printed, because
    /// the lead is woken when the watch exits and needs a line that says why.
    TeamClosed,
    /// The store cannot be read again by this binary (a newer or older
    /// schema, or not a teamwire database). `STORE-ERROR` was printed.
    StoreError,
    /// Another watch took over the lease.
    LeaseLost,
}

/// What the watch remembers from one poll to the next, in this process.
/// Everything that must outlive the process is in the store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchMemory {
    pub polls: u64,
    pub last_poll: Option<Timestamp>,
    /// The monotonic clock at `last_poll`. It stops while the machine
    /// sleeps and the wall clock does not, so the two differ by the sleep.
    pub(crate) last_mono: Option<Instant>,
    /// Set by the poll that decided the watch is over. The caller stops
    /// polling and releases the lease.
    pub end: Option<WatchEnd>,
    /// The team's name, once a poll has read it, for the events' prefix.
    pub(crate) team: Option<String>,
    pub(crate) lease_pid: Option<teamwire_core::Pid>,
    pub(crate) members: HashMap<Role, MemberMemory>,
    pub(crate) reported_errors: HashSet<&'static str>,
    /// After a sleep gap: quiet counting for every member starts again from
    /// here, as if each had just started.
    pub(crate) grace_from: Option<Timestamp>,
    /// Each sleep, with the time of the poll that found it: a deadline
    /// measures work, so a run subtracts only the sleeps after its start.
    pub(crate) sleeps: Vec<(Timestamp, Duration)>,
}

impl WatchMemory {
    /// A fresh memory that already knows the team's name, so a first poll
    /// that finds no team row can still name it in its `TEAM-CLOSED` line.
    pub fn for_team(team: &str) -> Self {
        Self {
            team: Some(team.to_owned()),
            ..Self::default()
        }
    }
}

/// One member's counters, for one generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MemberMemory {
    pub generation: u32,
    /// Consecutive polls seen idle with no completion.
    pub quiet: u32,
    /// Consecutive polls the member's session was reported gone.
    pub absent: u32,
    pub seen_working: bool,
    /// Polls a completion has been recorded but not reported.
    pub declared_polls: u32,
    /// The `SILENT` episode a quiet stretch belongs to, from 1.
    pub silent_ep: u32,
    /// The member has been announced silent in `silent_ep`, so the next
    /// time it works starts a new episode.
    pub silent_open: bool,
    pub blocked_ep: u32,
    pub blocked_open: bool,
}

impl MemberMemory {
    /// A member seen for the first time in `generation`, resuming after the
    /// episodes the store says were already announced.
    pub(crate) fn resumed(generation: u32, silent_seen: u32, blocked_seen: u32) -> Self {
        Self {
            generation,
            silent_ep: silent_seen.max(1),
            silent_open: silent_seen > 0,
            blocked_ep: blocked_seen.max(1),
            blocked_open: blocked_seen > 0,
            ..Self::default()
        }
    }

    /// Forgets what a quiet stretch counted: a sleep gap, not an event.
    pub(crate) fn forget_quiet(&mut self) {
        self.quiet = 0;
        self.absent = 0;
        self.declared_polls = 0;
        self.seen_working = false;
    }
}

/// The sleep time that falls inside a run that began at `run_start`. A sleep
/// found by a poll after the start can have begun before it, so it counts
/// for no more than the time since the start.
pub(crate) fn slept_since(sleeps: &[(Timestamp, Duration)], run_start: Timestamp) -> Duration {
    sleeps
        .iter()
        .map(|(woke, slept)| (*slept).min(woke.saturating_since(run_start)))
        .sum()
}
