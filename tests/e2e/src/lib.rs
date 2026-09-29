//! End-to-end tests: the real `teamwire` binary, real git, `fake-herdr` and
//! `fake-claude` on a test-built `PATH`, and a temporary `TEAMWIRE_HOME`
//! (`docs/test-strategy.md` section 5). The scenarios live in `tests/`; this
//! library is the world they run in.
//!
//! Never: run against a real herdr or Claude session, or a real home; leave a
//! fake process running when a test ends, on any path.

#![expect(
    clippy::disallowed_methods,
    reason = "the harness starts the teamwire binary, git and cargo as processes, and reads the monotonic clock to bound its waits"
)]
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a harness helper that cannot proceed fails the test that called it"
)]

mod bin;
pub mod sh;
mod watch;
mod world;

pub use watch::Watch;
pub use world::{Run, World, wait_until};
