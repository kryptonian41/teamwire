//! The contract suites: what every adapter of a trait must do, whatever the
//! product behind it. `host_contract`, `harness_contract` and
//! `platform_contract` take a small setup record because a suite cannot know
//! how to make a place, a session or a process for a given product.
//!
//! Each suite panics with a message naming the broken rule, so a caller uses
//! it as one test body. A suite checks only facts every adapter shares. It
//! never assumes herdr, tmux, Claude Code or one operating system.

mod harness;
mod host;
mod lead_setup;
mod permission;
mod platform;

pub use harness::{HarnessSetup, harness_contract};
pub use host::{HostSetup, host_contract};
pub use lead_setup::{LeadSetupSetup, lead_setup_contract};
pub use permission::{PermissionSetup, permission_contract};
pub use platform::{EnvProbe, PlatformSetup, RunCase, RunExpect, platform_contract};

use std::fmt::Debug;

/// Fails the calling test. A suite reports a broken rule by panicking, the
/// same way `assert!` does.
#[expect(
    clippy::panic,
    reason = "a contract suite fails the test that called it by panicking, like assert!"
)]
pub(crate) fn fail(rule: &str, detail: impl Debug) -> ! {
    panic!("contract violated: {rule}: {detail:?}")
}

/// Unwraps an `Ok`, or fails the contract naming the call that broke it.
pub(crate) fn ok<T, E: Debug>(call: &str, result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(e) => fail(&format!("{call} must succeed here"), e),
    }
}

/// Fails unless `cond` holds.
pub(crate) fn require(rule: &str, cond: bool) {
    if !cond {
        fail(rule, "");
    }
}
