//! A lead with no host (DEL-17): results go through the watch.
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

use teamwire_core::{EventType, LeadRoute, ReportState, TurnEndReply};
use teamwire_report::on_turn_end;

use kit::{stop, team};

#[test]
fn no_host_lead_gets_results_through_watch() {
    let (mut rig, api) = team();
    // Launch recorded that the lead has nowhere to type.
    rig.store
        .write(|tx| {
            tx.connection().execute(
                "UPDATE team SET lead_route = ?1",
                [LeadRoute::Watch.as_str()],
            )?;
            Ok(())
        })
        .unwrap();
    rig.write_report(&api, "Decisions I made: none\n");
    rig.complete_now(&api);
    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    );

    // The result file is written, nothing is typed, and the completion waits
    // as `pending` for the watch's COMPLETED line.
    assert!(rig.team_dir().join("outbox/api.md").exists());
    assert!(rig.typed_to_lead().is_empty());
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Pending
    );
    assert!(rig.events(EventType::ResultDelivering).is_empty());
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

fn watch_route(rig: &mut rig::Rig) {
    rig.store
        .write(|tx| {
            tx.connection().execute(
                "UPDATE team SET lead_route = ?1",
                [LeadRoute::Watch.as_str()],
            )?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn watch_route_done_writes_the_result_file_at_once() {
    let (mut rig, api) = team();
    watch_route(&mut rig);
    rig.complete_now(&api);
    // The watch may print `Result: outbox/api.md` before any Stop hook runs.
    let text = std::fs::read_to_string(rig.team_dir().join("outbox/api.md")).unwrap();
    assert!(
        text.starts_with("<!-- teamwire:generated completion="),
        "{text}"
    );
    assert!(text.contains("not available yet"), "{text}");
    assert!(text.contains("finished the api"), "{text}");
}

#[test]
fn final_message_is_recorded_after_the_watch_reported_the_result() {
    let (mut rig, api) = team();
    watch_route(&mut rig);
    let id = rig.complete_now(&api);
    // The watch's next poll marks the completion delivered.
    let at = kit::at(40);
    rig.store
        .write(|tx| tx.deliver_completion_via_watch(at, &id).map(|_| ()))
        .unwrap();

    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    );

    let record = rig.newest(&api);
    assert_eq!(record.completion.report_state, ReportState::Delivered);
    assert_eq!(record.completion.route, Some(teamwire_core::Route::Watch));
    assert_eq!(
        record.completion.final_source,
        teamwire_core::FinalSource::Hook
    );
    let text = std::fs::read_to_string(rig.team_dir().join("outbox/api.md")).unwrap();
    assert!(text.contains("All done, the api is finished."), "{text}");
    assert!(!text.contains("not available yet"), "{text}");
    assert!(
        rig.typed_to_lead().is_empty(),
        "nothing is typed for a watch result"
    );
    assert_eq!(rig.events(EventType::ResultDelivering).len(), 0);

    // Once recorded, a later stop leaves it alone.
    let before = std::fs::read(rig.team_dir().join("outbox/api.md")).unwrap();
    on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));
    assert_eq!(
        std::fs::read(rig.team_dir().join("outbox/api.md")).unwrap(),
        before
    );
}
