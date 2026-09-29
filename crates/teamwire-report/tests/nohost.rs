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
