#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use std::time::Duration;

use teamwire_core::{
    CompletionStatus, DeliveryError, DeliveryErrorCode, EventType, FinalSource, ReportState, Route,
    Ulid,
};

use crate::testkit::{at, count_events, last_event, role, with_members};
use crate::{NewCompletion, ResultOutcome};

const STALE: Duration = Duration::from_secs(90);

fn done(f: &mut crate::testkit::Fixture, secs: i64) -> Ulid {
    f.store
        .write(|tx| {
            tx.insert_completion(
                at(secs),
                &NewCompletion {
                    role: role("api"),
                    generation: 1,
                    status: CompletionStatus::Done,
                    summary: "all green".to_owned(),
                    final_message: Some("bye".to_owned()),
                    final_source: FinalSource::Hook,
                },
            )
        })
        .unwrap()
        .ulid
}

fn record(f: &crate::testkit::Fixture, c: &Ulid) -> crate::CompletionRecord {
    f.store.read(|tx| tx.completion(c)).unwrap().unwrap()
}

fn delivered(route: Route) -> ResultOutcome {
    ResultOutcome {
        state: ReportState::Delivered,
        route: Some(route),
        error: None,
        result_sha256: Some([9; 32]),
    }
}

#[test]
fn insert_numbers_per_member_and_writes_member_completed() {
    let mut f = with_members(&["api", "web"]);
    let first = done(&mut f, 10);
    let second = done(&mut f, 20);
    let r1 = record(&f, &first);
    let r2 = record(&f, &second);
    assert_eq!((r1.completion.n, r2.completion.n), (1, 2));
    assert_eq!(r1.completion.report_state, ReportState::Pending);
    assert_eq!(r1.completion.final_message.as_deref(), Some("bye"));
    let web = f
        .store
        .write(|tx| {
            tx.insert_completion(
                at(30),
                &NewCompletion {
                    role: role("web"),
                    generation: 1,
                    status: CompletionStatus::Blocked,
                    summary: "s".into(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
        })
        .unwrap();
    assert_eq!(web.n, 1, "the numbers are per member");
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::MemberCompleted);
    assert_eq!(e.subject, Some(web.ulid));
    assert_eq!(e.data["status"], "blocked");
    assert_eq!(e.data["n"], 1);
    assert_eq!(count_events(&f.store, EventType::MemberCompleted), 3);
}

#[test]
fn only_the_first_claim_wins() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    let first = f
        .store
        .write(|tx| tx.claim_completion_for_delivery(at(11), &c, STALE))
        .unwrap();
    assert_eq!(first, Some(ReportState::Pending));
    let second = f
        .store
        .write(|tx| tx.claim_completion_for_delivery(at(12), &c, STALE))
        .unwrap();
    assert_eq!(second, None, "a live claim was taken twice");
    let r = record(&f, &c);
    assert_eq!(r.completion.report_state, ReportState::Delivering);
    assert_eq!(r.delivering_since, Some(at(11)));
    assert_eq!(count_events(&f.store, EventType::ResultDelivering), 1);
    assert_eq!(last_event(&f.store).data["seconds"], 1.0);
}

#[test]
fn a_stale_delivering_result_can_be_claimed_again() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(11), &c, STALE))
        .unwrap();
    let too_soon = f
        .store
        .write(|tx| tx.claim_completion_for_delivery(at(100), &c, STALE))
        .unwrap();
    assert_eq!(too_soon, None);
    let again = f
        .store
        .write(|tx| tx.claim_completion_for_delivery(at(102), &c, STALE))
        .unwrap();
    assert_eq!(again, Some(ReportState::Delivering));
    assert_eq!(record(&f, &c).delivering_since, Some(at(102)));
}

#[test]
fn finishing_needs_delivering_and_writes_the_outcome_event() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    // Not claimed yet: nothing to finish.
    assert!(
        !f.store
            .write(|tx| tx.finish_completion_delivery(at(11), &c, &delivered(Route::HostTyping)))
            .unwrap()
    );
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(12), &c, STALE))
        .unwrap();
    assert!(
        f.store
            .write(|tx| tx.finish_completion_delivery(at(13), &c, &delivered(Route::HostTyping)))
            .unwrap()
    );
    let r = record(&f, &c);
    assert_eq!(
        (r.completion.report_state, r.completion.route),
        (ReportState::Delivered, Some(Route::HostTyping))
    );
    assert_eq!(r.result_sha256, Some([9; 32]));
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::ResultDelivered);
    assert_eq!(e.data["seconds"], 3.0);
    // Finishing twice, or claiming a delivered result, does nothing.
    assert!(
        !f.store
            .write(|tx| tx.finish_completion_delivery(at(14), &c, &delivered(Route::Watch)))
            .unwrap()
    );
    assert_eq!(
        f.store
            .write(|tx| tx.claim_completion_for_delivery(at(200), &c, STALE))
            .unwrap(),
        None
    );
    assert_eq!(count_events(&f.store, EventType::ResultDelivered), 1);
}

#[test]
fn undelivered_and_unconfirmed_outcomes_keep_the_reason() {
    for (state, kind) in [
        (ReportState::Undelivered, EventType::ResultUndelivered),
        (ReportState::Unconfirmed, EventType::ResultUnconfirmed),
    ] {
        let mut f = with_members(&["api"]);
        let c = done(&mut f, 10);
        f.store
            .write(|tx| tx.claim_completion_for_delivery(at(11), &c, STALE))
            .unwrap();
        let error = DeliveryError::new(DeliveryErrorCode::BudgetExhausted, "40 s used");
        let outcome = ResultOutcome {
            state,
            route: None,
            error: Some(error.clone()),
            result_sha256: None,
        };
        assert!(
            f.store
                .write(|tx| tx.finish_completion_delivery(at(12), &c, &outcome))
                .unwrap()
        );
        let r = record(&f, &c);
        assert_eq!(r.completion.reason, Some(error));
        assert_eq!(last_event(&f.store).kind, kind);
        assert_eq!(last_event(&f.store).data["code"], "budget_exhausted");
    }
}

#[test]
fn a_delivery_cannot_end_as_pending_or_delivering() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(11), &c, STALE))
        .unwrap();
    for state in [ReportState::Pending, ReportState::Delivering] {
        let outcome = ResultOutcome {
            state,
            route: None,
            error: None,
            result_sha256: None,
        };
        assert!(
            f.store
                .write(|tx| tx.finish_completion_delivery(at(12), &c, &outcome))
                .is_err()
        );
    }
}

#[test]
fn an_undelivered_result_can_be_claimed_again() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(11), &c, STALE))
        .unwrap();
    let outcome = ResultOutcome {
        state: ReportState::Undelivered,
        route: None,
        error: None,
        result_sha256: None,
    };
    f.store
        .write(|tx| tx.finish_completion_delivery(at(12), &c, &outcome))
        .unwrap();
    let again = f
        .store
        .write(|tx| tx.claim_completion_for_delivery(at(20), &c, STALE))
        .unwrap();
    assert_eq!(again, Some(ReportState::Undelivered));
}

#[test]
fn the_lead_marks_an_undelivered_or_unconfirmed_result_received() {
    for state in [ReportState::Undelivered, ReportState::Unconfirmed] {
        let mut f = with_members(&["api"]);
        let c = done(&mut f, 10);
        // A pending result has not been tried, so the lead cannot claim it.
        assert!(
            !f.store
                .write(|tx| tx.mark_result_received(at(11), &c))
                .unwrap()
        );
        f.store
            .write(|tx| tx.claim_completion_for_delivery(at(11), &c, STALE))
            .unwrap();
        let error = DeliveryError::new(DeliveryErrorCode::SessionBlocked, "a dialog");
        let outcome = ResultOutcome {
            state,
            route: None,
            error: Some(error.clone()),
            result_sha256: None,
        };
        f.store
            .write(|tx| tx.finish_completion_delivery(at(12), &c, &outcome))
            .unwrap();
        assert!(
            f.store
                .write(|tx| tx.mark_result_received(at(40), &c))
                .unwrap()
        );
        let r = record(&f, &c);
        assert_eq!(
            (r.completion.report_state, r.completion.route),
            (ReportState::Delivered, Some(Route::Inbox))
        );
        assert_eq!(
            r.completion.reason,
            Some(error),
            "why the typing failed stays"
        );
        let e = last_event(&f.store);
        assert_eq!(e.kind, EventType::ResultDelivered);
        assert_eq!(e.data["route"], "inbox");
        assert_eq!(e.data["seconds"], 30.0);
        assert!(
            !f.store
                .write(|tx| tx.mark_result_received(at(41), &c))
                .unwrap(),
            "a second mark changes nothing"
        );
        assert_eq!(count_events(&f.store, EventType::ResultDelivered), 1);
    }
}

#[test]
fn the_watch_interrupts_a_stale_delivering_result_once() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    // Not delivering yet.
    assert!(
        !f.store
            .write(|tx| tx.interrupt_stale_delivery(at(500), &c, STALE))
            .unwrap()
    );
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(100), &c, STALE))
        .unwrap();
    // Exactly at the limit and before it: not yet.
    assert!(
        !f.store
            .write(|tx| tx.interrupt_stale_delivery(at(190), &c, STALE))
            .unwrap()
    );
    assert!(
        f.store
            .write(|tx| tx.interrupt_stale_delivery(at(191), &c, STALE))
            .unwrap()
    );
    let r = record(&f, &c);
    assert_eq!(r.completion.report_state, ReportState::Undelivered);
    assert_eq!(
        r.completion.reason.map(|e| e.code),
        Some(DeliveryErrorCode::DeliveryInterrupted)
    );
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::ResultUndelivered);
    assert_eq!(e.data["code"], "delivery_interrupted");
    // A second watch finds nothing to do, and the hook's late finish is refused.
    assert!(
        !f.store
            .write(|tx| tx.interrupt_stale_delivery(at(200), &c, STALE))
            .unwrap()
    );
    assert!(
        !f.store
            .write(|tx| tx.finish_completion_delivery(at(201), &c, &delivered(Route::HostTyping)))
            .unwrap()
    );
    assert_eq!(count_events(&f.store, EventType::ResultUndelivered), 1);
}

#[test]
fn reads_by_member_latest_and_state() {
    let mut f = with_members(&["api"]);
    let first = done(&mut f, 10);
    let second = done(&mut f, 20);
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(21), &first, STALE))
        .unwrap();
    let all = f
        .store
        .read(|tx| tx.completions_of(&role("api")))
        .unwrap()
        .items;
    assert_eq!(
        all.iter().map(|c| c.completion.n).collect::<Vec<_>>(),
        [1, 2]
    );
    let latest = f
        .store
        .read(|tx| tx.latest_completion(&role("api")))
        .unwrap()
        .unwrap();
    assert_eq!(latest.completion.ulid, second);
    let pending = f
        .store
        .read(|tx| tx.completions_in_states(&[ReportState::Pending]))
        .unwrap()
        .items;
    assert_eq!(pending.len(), 1);
    assert_eq!(
        f.store
            .read(|tx| tx.latest_completion(&role("nobody")))
            .unwrap(),
        None
    );
}

#[test]
fn the_final_message_is_recorded_on_an_existing_completion() {
    let mut f = with_members(&["api"]);
    let c = f
        .store
        .write(|tx| {
            tx.insert_completion(
                at(10),
                &NewCompletion {
                    role: role("api"),
                    generation: 1,
                    status: CompletionStatus::Done,
                    summary: "s".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
        })
        .unwrap()
        .ulid;
    let events = crate::testkit::event_types(&f.store).len();
    assert!(
        f.store
            .write(|tx| tx.set_completion_final(&c, Some("all done"), FinalSource::Hook))
            .unwrap()
    );
    let r = record(&f, &c);
    assert_eq!(r.completion.final_message.as_deref(), Some("all done"));
    assert_eq!(r.completion.final_source, FinalSource::Hook);
    assert_eq!(r.completion.report_state, ReportState::Pending);
    assert_eq!(crate::testkit::event_types(&f.store).len(), events);
    let missing = crate::testkit::fixture()
        .store
        .write(|tx| tx.set_completion_final(&c, None, FinalSource::None))
        .unwrap();
    assert!(!missing);
}

#[test]
fn route_b_moves_pending_straight_to_delivered_with_one_event() {
    let mut f = with_members(&["api"]);
    let first = done(&mut f, 10);
    assert!(
        f.store
            .write(|tx| tx.deliver_completion_via_watch(at(15), &first))
            .unwrap()
    );
    let r = record(&f, &first);
    assert_eq!(
        (r.completion.report_state, r.completion.route),
        (ReportState::Delivered, Some(Route::Watch))
    );
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::ResultDelivered);
    assert_eq!(e.data["seconds"], 5.0);
    assert_eq!(count_events(&f.store, EventType::ResultDelivering), 0);
    // Only a pending completion moves this way.
    assert!(
        !f.store
            .write(|tx| tx.deliver_completion_via_watch(at(16), &first))
            .unwrap()
    );
    let second = done(&mut f, 20);
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(21), &second, STALE))
        .unwrap();
    assert!(
        !f.store
            .write(|tx| tx.deliver_completion_via_watch(at(22), &second))
            .unwrap()
    );
    assert_eq!(count_events(&f.store, EventType::ResultDelivered), 1);
}

#[test]
fn a_hook_run_puts_its_duration_on_the_outcome_event() {
    let mut f = with_members(&["api"]);
    let c = done(&mut f, 10);
    f.store
        .write(|tx| tx.claim_completion_for_delivery(at(12), &c, STALE))
        .unwrap();
    let extra = serde_json::json!({ "hook": "claude-stop", "hook_ms": 850, "seconds": -1 });
    assert!(
        f.store
            .write(|tx| {
                tx.finish_completion_delivery_with(
                    at(13),
                    &c,
                    &delivered(Route::HostTyping),
                    &extra,
                )
            })
            .unwrap()
    );
    let e = last_event(&f.store);
    assert_eq!(e.kind, EventType::ResultDelivered);
    assert_eq!(
        (&e.data["hook"], &e.data["hook_ms"]),
        (&serde_json::json!("claude-stop"), &serde_json::json!(850))
    );
    // The method's own fields win over the caller's.
    assert_eq!(e.data["seconds"], 3.0);
}
