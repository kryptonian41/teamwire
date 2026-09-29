//! herdr's error codes and how `agent start` retries (ADP-5, LCH-13, LCH-25).
#![expect(
    clippy::expect_used,
    reason = "helpers in an integration-test file unwrap and index; a failure there fails the test"
)]

#[path = "support/helpers.rs"]
mod helpers;

use std::time::Duration;

use helpers::{T, World};
use teamwire_core::one_line;
use teamwire_core::{Host, HostErrorCode, OneLine, Place};
use teamwire_fakes::HerdrFaults;

fn place(world: &World, id: &str) -> Place {
    Place {
        host: world.herdr.id(),
        id: id.to_owned(),
    }
}

/// A world where every `command` (with `target`) fails with `code`.
fn failing(command: &str, code: &str) -> World {
    World::with_faults(Some(&HerdrFaults::new().error(command, None, None, code)))
}

fn spawn_code(code: &str) -> teamwire_core::HostError {
    let world = failing("agent start", code);
    world
        .herdr
        .spawn(&world.request("tw_errs_a1", "one"), T)
        .expect_err("start is failed by the fault")
}

fn prompt_code(code: &str) -> teamwire_core::HostError {
    let world = failing("agent prompt", code);
    let text: OneLine = one_line("hello", 20);
    world
        .herdr
        .submit(&place(&world, "w1:p1"), &text, T)
        .expect_err("prompt is failed by the fault")
}

#[test]
fn pane_busy_retried_by_code() {
    let world = World::with_faults(Some(&HerdrFaults::new().error(
        "agent start",
        None,
        Some(2),
        "agent_pane_busy",
    )));
    let placed = world
        .herdr
        .spawn(&world.request("tw_errs_a1", "one"), T)
        .expect("busy twice, then the third try starts");
    assert!(!placed.id.is_empty());
    assert_eq!(world.calls_of("agent start").len(), 3);
}

#[test]
fn pane_busy_forever_gives_up_after_the_bound_with_busy() {
    let world = failing("agent start", "agent_pane_busy");
    let e = world
        .herdr
        .spawn(&world.request("tw_errs_a1", "one"), T)
        .expect_err("never starts");
    assert_eq!(e.code, HostErrorCode::Busy);
    assert_eq!(world.calls_of("agent start").len(), 8);
}

#[test]
fn not_ready_returns_the_place_without_retry() {
    let world = failing("agent start", "agent_not_ready");
    let placed = world
        .herdr
        .spawn(&world.request("tw_errs_a1", "one"), T)
        .expect("a not-ready agent is started and waits at a dialog");
    assert!(!placed.id.is_empty());
    assert_eq!(world.calls_of("agent start").len(), 1);
}

#[test]
fn other_start_errors_are_not_retried() {
    let e = spawn_code("invalid_agent_argument");
    assert_eq!(e.code, HostErrorCode::BadArgument);
    let world = failing("agent start", "invalid_agent_argument");
    world
        .herdr
        .spawn(&world.request("tw_errs_a1", "one"), T)
        .expect_err("refused");
    assert_eq!(world.calls_of("agent start").len(), 1);
}

#[test]
fn busy_is_not_retried_outside_agent_start() {
    let world = failing("agent prompt", "agent_pane_busy");
    let text = one_line("hello", 20);
    let e = world
        .herdr
        .submit(&place(&world, "w1:p1"), &text, T)
        .expect_err("busy");
    assert_eq!(e.code, HostErrorCode::Busy);
    assert_eq!(world.calls_of("agent prompt").len(), 1);
}

#[test]
fn code_agent_blocked_is_blocked() {
    assert_eq!(prompt_code("agent_blocked").code, HostErrorCode::Blocked);
}

#[test]
fn code_agent_prompt_stalled_is_stalled() {
    assert_eq!(
        prompt_code("agent_prompt_stalled").code,
        HostErrorCode::Stalled
    );
}

#[test]
fn code_timeout_is_timeout() {
    assert_eq!(prompt_code("timeout").code, HostErrorCode::Timeout);
}

#[test]
fn not_found_codes_are_not_found() {
    for code in [
        "agent_not_found",
        "pane_not_found",
        "agent_pane_not_found",
        "workspace_not_found",
        "tab_not_found",
    ] {
        assert_eq!(prompt_code(code).code, HostErrorCode::NotFound, "{code}");
    }
}

#[test]
fn bad_argument_codes_are_bad_argument() {
    for code in [
        "invalid_agent_argument",
        "invalid_agent_name",
        "invalid_agent_timeout",
        "empty_agent_prompt",
        "invalid_params",
    ] {
        assert_eq!(prompt_code(code).code, HostErrorCode::BadArgument, "{code}");
    }
}

#[test]
fn agent_name_taken_is_bad_argument_with_the_code_first_in_the_detail() {
    let e = spawn_code("agent_name_taken");
    assert_eq!(e.code, HostErrorCode::BadArgument);
    assert!(e.detail.starts_with("agent_name_taken:"), "{}", e.detail);
}

#[test]
fn unknown_code_is_other_and_keeps_the_code() {
    let e = prompt_code("agent_launch_pending");
    assert_eq!(e.code, HostErrorCode::Other);
    assert!(e.detail.contains("agent_launch_pending"), "{}", e.detail);
}

#[test]
fn the_message_text_never_decides_the_code() {
    // The fake's message for a fault is its own words; a code that maps to
    // `Other` stays `Other` however its message reads.
    let e = prompt_code("something_new");
    assert_eq!(e.code, HostErrorCode::Other);
}

#[test]
fn a_call_that_hangs_is_a_timeout() {
    let world = World::with_faults(Some(&HerdrFaults::new().hang("agent get", None, None)));
    let e = world
        .herdr
        .state(&place(&world, "w1:p1"), Duration::from_millis(300))
        .expect_err("hangs");
    assert_eq!(e.code, HostErrorCode::Timeout);
}

#[test]
fn garbage_output_is_other() {
    let world = World::with_faults(Some(&HerdrFaults::new().garbage("agent list", None, None)));
    let e = world.herdr.live_agent_names(T).expect_err("not json");
    assert_eq!(e.code, HostErrorCode::Other);
}

#[test]
fn a_live_name_from_the_fake_itself_carries_the_taken_code() {
    let world = World::new();
    world
        .herdr
        .spawn(&world.request("tw_errs_a1", "one"), T)
        .expect("first start");
    let e = world
        .herdr
        .spawn(&world.request("tw_errs_a1", "two"), T)
        .expect_err("name is live");
    assert_eq!(e.code, HostErrorCode::BadArgument);
    assert!(e.detail.starts_with("agent_name_taken:"), "{}", e.detail);
}
