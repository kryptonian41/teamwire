//! The report check: who is told about report gaps, and how often.
#![expect(
    clippy::unwrap_used,
    reason = "a failing unwrap in an integration test fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/kit.rs"]
mod kit;
#[path = "../../teamwire-deliver/tests/support/members.rs"]
mod members;
#[path = "../../teamwire-deliver/tests/support/rig.rs"]
mod rig;

use teamwire_core::{CompletionStatus, EventType};
use teamwire_report::{DoneReport, done};

use kit::team;

#[test]
fn no_hook_harness_told_once() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();

    // No report file at all: the first call stores nothing and says why.
    let first = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    let DoneReport::NotStored {
        stored,
        gaps,
        report,
    } = first
    else {
        unreachable!("the first call must not store")
    };
    assert!(!stored);
    assert!(gaps.report_missing);
    assert!(report.contains("reports/api.md"), "{report}");
    assert_eq!(rig.completions(&api), 0);
    assert_eq!(rig.events(EventType::MemberHeld).len(), 1);

    // The second call stores whatever the report says.
    let second = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    assert!(matches!(second, DoneReport::Stored { n: 1, .. }));
    assert_eq!(rig.completions(&api), 1);
}

#[test]
fn no_hook_harness_with_a_complete_report_is_stored_at_once() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();
    rig.write_report(&api, "notes\n\nDecisions I made: none\n");
    let report = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    assert!(matches!(report, DoneReport::Stored { .. }));
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn a_harness_with_a_turn_end_hook_is_not_checked_by_done() {
    let (mut rig, api) = team();
    // No report file, but the Stop hook will run the check at turn end.
    let report = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    assert!(matches!(report, DoneReport::Stored { .. }));
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn a_new_generation_is_told_again() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();
    let told = |rig: &mut rig::Rig| {
        matches!(
            done(&mut rig.ctx(), &api, CompletionStatus::Done, "x").unwrap(),
            DoneReport::NotStored { .. }
        )
    };
    assert!(told(&mut rig));
    assert!(!told(&mut rig));
    rig.store
        .write(|tx| tx.bump_generation(kit::at(9), &api, "again").map(|_| ()))
        .unwrap();
    assert!(told(&mut rig), "generation 2 is told once more");
}
