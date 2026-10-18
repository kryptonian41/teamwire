//! Route A of ADR 0007: what the Stop hook does at the end of a member's turn.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a failing unwrap or index in an integration test fails the test"
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

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::Digest as _;
use teamwire_core::{
    Clock, DeliveryErrorCode, EventType, FinalSource, HostError, HostErrorCode, Pid, ReportState,
    Role, TeamName, TurnEndReply,
};
use teamwire_fakes::Fault;
use teamwire_report::on_turn_end;
use teamwire_store::Store;

use kit::{at, data_str, stop, team};
use rig::Rig;

/// Each host call during a delivery: its name, how long a second writer waited, and whether it wrote.
type Seen = Arc<Mutex<Vec<(&'static str, Duration, bool)>>>;

const COMPLETE_REPORT: &str = "Progress.\n\nDecisions I made: none\n";

fn finished_member() -> (Rig, Role) {
    let (rig, api) = team();
    rig.write_report(&api, COMPLETE_REPORT);
    (rig, api)
}

fn reply(rig: &mut Rig, api: &Role) -> TurnEndReply {
    on_turn_end(&mut rig.ctx(), api, &stop("api-1"))
}

fn take_lease(rig: &mut Rig, pid: u32) {
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.take_watch_lease(now, Pid::new(pid), "host", |_| true)
                .map(|_| ())
        })
        .unwrap();
}

#[test]
fn a_result_without_a_running_watch_says_so() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    let lines = rig.typed_to_lead();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with(" (no watch is running: teamwire watch alpha)"),
        "{}",
        lines[0]
    );
}

#[test]
fn a_lead_that_cannot_read_watch_lines_is_told_the_background_watch() {
    let (mut rig, api) = finished_member();
    rig.harness.set_caps(teamwire_core::HarnessCaps {
        reads_watch_lines: false,
        ..teamwire_fakes::FakeHarness::caps()
    });
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert!(
        rig.typed_to_lead()[0].ends_with(
            " (no watch is running: nohup teamwire watch alpha --deliver >/dev/null 2>&1 &)"
        ),
        "{}",
        rig.typed_to_lead()[0]
    );
}

#[test]
fn a_result_with_a_dead_watch_says_so() {
    let (mut rig, api) = finished_member();
    take_lease(&mut rig, 700); // the process is not alive
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert!(rig.typed_to_lead()[0].ends_with("(no watch is running: teamwire watch alpha)"));
}

#[test]
fn a_result_with_a_running_watch_carries_no_note() {
    let (mut rig, api) = finished_member();
    rig.platform.set_alive(Pid::new(700), true);
    take_lease(&mut rig, 700);
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    let lines = rig.typed_to_lead();
    assert!(!lines[0].contains("no watch is running"), "{}", lines[0]);
}

#[test]
fn the_note_stays_on_the_longest_result_line() {
    let (mut rig, api) = finished_member();
    teamwire_report::done(
        &mut rig.ctx(),
        &api,
        teamwire_core::CompletionStatus::Done,
        &"x ".repeat(900),
    )
    .unwrap();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    let line = &rig.typed_to_lead()[0];
    assert!(
        line.ends_with("(no watch is running: teamwire watch alpha)"),
        "{line}"
    );
    assert!(line.contains("Full result:"), "{line}");
}

#[test]
fn delivers_the_result_and_writes_the_file_first() {
    let (mut rig, api) = finished_member();
    let id = rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);

    let lines = rig.typed_to_lead();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("[teamwire:alpha] api finished (done). finished the api"),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("outbox/api.md"), "{}", lines[0]);

    let record = rig.newest(&api);
    assert_eq!(record.completion.report_state, ReportState::Delivered);
    assert_eq!(record.completion.final_source, FinalSource::Hook);
    assert_eq!(
        record.completion.final_message.as_deref(),
        Some("All done, the api is finished.")
    );
    let file = std::fs::read(rig.team_dir().join("outbox/api.md")).unwrap();
    let text = String::from_utf8(file.clone()).unwrap();
    assert!(
        text.starts_with(&format!("<!-- teamwire:generated completion={id} -->")),
        "{text}"
    );
    assert!(text.contains("All done, the api is finished."));
    let digest: [u8; 32] = sha2::Sha256::digest(&file).into();
    assert_eq!(record.result_sha256, Some(digest));

    // The file was written before the claim, and the outcome event carries
    // the hook's name and duration.
    let delivered = &rig.events(EventType::ResultDelivered)[0];
    assert_eq!(data_str(delivered, "hook"), Some("fake-stop"));
    assert!(delivered.data.get("hook_ms").is_some());
    assert_eq!(rig.events(EventType::ResultDelivering).len(), 1);
}

#[test]
fn second_stop_reports_nothing() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    reply(&mut rig, &api);
    let before = std::fs::read(rig.team_dir().join("outbox/api.md")).unwrap();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);
    assert_eq!(rig.events(EventType::ResultDelivering).len(), 1);
    assert_eq!(
        std::fs::read(rig.team_dir().join("outbox/api.md")).unwrap(),
        before
    );
}

#[test]
fn a_stop_with_no_completion_reports_nothing() {
    let (mut rig, api) = finished_member();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert!(rig.typed_to_lead().is_empty());
    assert!(!rig.team_dir().join("outbox/api.md").exists());
}

#[test]
fn unreachable_lead_marks_undelivered() {
    let mut rig = Rig::new();
    // No place holds the lead's session.
    let teamwire_store::Party::Member(api) = rig.add_member("api", "fake", "api-1") else {
        unreachable!()
    };
    rig.write_report(&api, COMPLETE_REPORT);
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);

    let record = rig.newest(&api);
    assert_eq!(record.completion.report_state, ReportState::Undelivered);
    assert!(record.completion.reason.is_some());
    assert!(
        rig.team_dir().join("outbox/api.md").exists(),
        "the file is written first"
    );
    assert_eq!(rig.events(EventType::ResultUndelivered).len(), 1);
    // Nothing more is typed on the next stop: the watch announces it.
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.events(EventType::ResultDelivering).len(), 1);
}

#[test]
fn completion_after_reprompt_is_new_result() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    reply(&mut rig, &api);

    // A reprompt: generation 2. Its stop with no new completion reports nothing.
    rig.store
        .write(|tx| tx.bump_generation(at(20), &api, "more work").map(|_| ()))
        .unwrap();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);

    rig.complete_now(&api);
    reply(&mut rig, &api);
    let lines = rig.typed_to_lead();
    assert_eq!(lines.len(), 2);
    let newest = rig.newest(&api);
    assert_eq!((newest.completion.n, newest.completion.generation), (2, 2));
    assert_eq!(newest.completion.report_state, ReportState::Delivered);
}

#[test]
fn no_hold_when_hook_already_active() {
    let (mut rig, api) = team(); // no report at all
    rig.complete_now(&api);
    let mut t = stop("api-1");
    t.hold_already_active = true;
    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &t),
        TurnEndReply::Continue
    );
    assert_eq!(rig.typed_to_lead().len(), 1, "delivered, not held");
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn other_session_is_not_member() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    let reply = on_turn_end(&mut rig.ctx(), &api, &stop("someone-else"));
    assert_eq!(reply, TurnEndReply::Continue);
    assert!(rig.typed_to_lead().is_empty());
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Pending
    );
    assert!(!rig.team_dir().join("outbox/api.md").exists());
}

#[test]
fn a_subagent_stop_reports_nothing() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    let mut t = stop("api-1");
    t.subagent = true;
    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &t),
        TurnEndReply::Continue
    );
    assert!(rig.typed_to_lead().is_empty());
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Pending
    );
}

#[test]
fn delivering_left_by_crash_delivered_once_more() {
    let (mut rig, api) = finished_member();
    let id = rig.complete_now(&api);
    // The earlier hook claimed the result and died before it typed anything.
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.claim_completion_for_delivery(now, &id, Duration::from_secs(60))
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Delivering
    );

    // The next turn end delivers it, with no waiting period.
    rig.clock.advance(Duration::from_millis(1));
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1, "delivered once more");
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Delivered
    );
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1, "and no more than once");
}

#[test]
fn hanging_host_stays_within_budget() {
    let (mut rig, api) = finished_member();
    let clock = rig.clock.clone();
    // Each host call takes 14 s on top of what it was given.
    rig.wrap_host(move |_, _| clock.advance(Duration::from_secs(14)));
    rig.complete_now(&api);
    let start = rig.clock.monotonic();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    let spent = rig.clock.monotonic().saturating_duration_since(start);
    assert!(
        spent < Duration::from_secs(60),
        "the hook must end before its 60 s limit: {spent:?}"
    );

    let record = rig.newest(&api);
    assert_eq!(record.completion.report_state, ReportState::Undelivered);
    assert_eq!(
        record.completion.reason.as_ref().map(|e| e.code),
        Some(DeliveryErrorCode::BudgetExhausted)
    );
    assert!(rig.typed_to_lead().is_empty(), "nothing was typed");
    let event = &rig.events(EventType::ResultUndelivered)[0];
    let ms = event.data["hook_ms"].as_u64().unwrap();
    assert!((28_000..60_000).contains(&ms), "{ms}");
}

#[test]
fn submit_timeout_is_unconfirmed_and_not_retried() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    rig.host.inject_always("submit", Fault::Hang);
    reply(&mut rig, &api);
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Unconfirmed
    );
    assert_eq!(rig.host.log().count_named("submit"), 1);
    reply(&mut rig, &api);
    assert_eq!(rig.host.log().count_named("submit"), 1, "never typed again");
}

#[test]
fn host_refusal_marks_undelivered_with_the_host_code() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    rig.host.inject_always(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Other, "refused")),
    );
    reply(&mut rig, &api);
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Undelivered
    );
}

#[test]
fn no_write_lock_held_during_lead_delivery() {
    let (mut rig, api) = finished_member();
    rig.complete_now(&api);
    let home = rig.home.path().to_path_buf();
    let seen: Seen = Arc::default();
    let log = seen.clone();
    rig.wrap_host(move |op, _| {
        // Another process writes while the hook waits on the host.
        let mut other = Store::open_existing(&home, &TeamName::parse("alpha").unwrap()).unwrap();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test measures real elapsed time to show that no write lock is held"
        )]
        let started = Instant::now();
        let ok = other
            .write(|tx| {
                tx.record_event(
                    at(50),
                    EventType::WatchLine,
                    None,
                    None,
                    &serde_json::json!({}),
                )
                .map(|_| ())
            })
            .is_ok();
        log.lock().unwrap().push((op, started.elapsed(), ok));
    });
    reply(&mut rig, &api);
    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|(op, _, _)| *op == "submit"), "{seen:?}");
    for (op, took, ok) in seen.iter() {
        assert!(*ok, "a write during {op} failed");
        assert!(
            *took < Duration::from_secs(1),
            "a write during {op} waited {took:?}"
        );
    }
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Delivered
    );
}
