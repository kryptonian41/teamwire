//! `kill -9` of the member's Stop hook while it delivers a result: the
//! completion is left `delivering`, and the next turn end delivers it once
//! more (`docs/test-strategy.md` section 3, ADR 0007).
#![expect(
    clippy::disallowed_methods,
    reason = "the test measures real elapsed time to place its kill points"
)]
#![expect(
    clippy::print_stderr,
    reason = "the kill window and the counts are printed so a failing run can be read"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::time::{Duration, Instant};

use rig::{Ended, Identity, Rig, assert_integrity};
use teamwire_core::{MemberKind, ReportState, Role};

const KILL_POINTS: usize = 40;

/// A team with a live lead and a member `api` whose session is
/// `session-api`. Nothing in `api`'s report is missing, because the hook is
/// told a hold is already active and so never holds.
fn team() -> Rig {
    let rig = Rig::new();
    rig.start_lead();
    rig.add_member("api", MemberKind::Implementer, false);
    rig
}

fn me() -> Identity {
    Identity::Member("api".to_owned())
}

const STOP: &str =
    r#"{"session_id":"session-api","hook_event_name":"Stop","stop_hook_active":true}"#;

fn hook_args() -> Vec<&'static str> {
    vec!["hook", "claude-stop", "--team", "alpha", "--role", "api"]
}

fn stop_hook(rig: &Rig) -> Ended {
    let out = rig.start_with_stdin(&hook_args(), &me(), STOP).finish();
    assert_eq!(out.code, Some(0), "{out:?}");
    out
}

fn record_done(rig: &Rig, summary: &str) {
    let out = rig.run(&["done", "--status", "done", "--summary", summary], &me());
    assert_eq!(out.code, Some(0), "{out:?}");
}

fn latest_state(rig: &Rig) -> ReportState {
    rig.store()
        .read(|tx| tx.latest_completion(&Role::parse("api").unwrap()))
        .unwrap()
        .unwrap()
        .completion
        .report_state
}

fn delivering(rig: &Rig) -> usize {
    rig.store()
        .read(|tx| tx.completions_of(&Role::parse("api").unwrap()))
        .unwrap()
        .items
        .iter()
        .filter(|c| c.completion.report_state == ReportState::Delivering)
        .count()
}

#[test]
fn kill9_between_delivering_and_outcome_delivers_once_more() {
    let mut rig = team();
    record_done(&rig, "between-claim-and-outcome");
    assert_eq!(latest_state(&rig), ReportState::Pending);

    // The hook has claimed the completion and typed the result into the
    // lead; the wrapper holds it there until it is killed.
    let signal = rig.pause_after("agent prompt");
    let mut running = rig.start_with_stdin(&hook_args(), &me(), STOP);
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    running.finish();
    rig.path_prefix = None;

    assert_integrity(&rig.store());
    assert_eq!(latest_state(&rig), ReportState::Delivering);
    assert_eq!(rig.typed_count("between-claim-and-outcome"), 1);

    // The next turn end delivers it again, once, and settles it.
    stop_hook(&rig);
    assert_eq!(latest_state(&rig), ReportState::Delivered);
    assert_eq!(rig.typed_count("between-claim-and-outcome"), 2);
    assert_eq!(delivering(&rig), 0);

    // A further turn end types nothing more.
    stop_hook(&rig);
    assert_eq!(rig.typed_count("between-claim-and-outcome"), 2);
    assert_integrity(&rig.store());
}

#[test]
fn kill9_of_the_stop_hook_at_random_points_leaves_nothing_delivering() {
    let mut rng = rig::Rng::from_env("kill9_of_the_stop_hook_at_random_points");
    let rig = team();
    record_done(&rig, "warm-up");
    let started = Instant::now();
    stop_hook(&rig);
    let window = started.elapsed() * 13 / 10;
    eprintln!("kill window {window:?}");

    for i in 0..KILL_POINTS {
        let summary = format!("hook-kill-{i}");
        record_done(&rig, &summary);
        let delay: Duration = rng.up_to(if i % 2 == 0 { window } else { window / 5 });
        let mut running = rig.start_with_stdin(&hook_args(), &me(), STOP);
        let begun = Instant::now();
        while begun.elapsed() < delay {
            std::hint::spin_loop();
        }
        running.kill9();
        running.finish();

        assert_integrity(&rig.store());
        let typed = rig.typed_count(&summary);
        assert!(typed <= 1, "{summary} typed {typed} times before the kill");
        let state = latest_state(&rig);
        assert!(
            matches!(
                state,
                ReportState::Pending | ReportState::Delivering | ReportState::Delivered
            ),
            "{summary}: {state:?} at {delay:?}"
        );

        // One more turn end settles it, and types it at most once more.
        stop_hook(&rig);
        assert_eq!(latest_state(&rig), ReportState::Delivered, "{summary}");
        assert_eq!(delivering(&rig), 0);
        let typed = rig.typed_count(&summary);
        assert!((1..=2).contains(&typed), "{summary} typed {typed} times");
    }
}
