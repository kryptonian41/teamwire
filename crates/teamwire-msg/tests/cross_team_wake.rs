//! A message from another team follows the wake and hold rules of a message
//! from the recipient's own team (ADR 0043 decisions 8 and 11): it wakes a
//! member whose turn ended with a result, and it is held for a member whose
//! newest result asked for a handoff.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;
#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/teams.rs"]
mod teams;

use teamwire_core::{
    CompletionStatus, DeliveryErrorCode, DeliveryState, EventType, FinalSource, MemberKind,
    MessageType, Place,
};
use teamwire_msg::{RecipientState, send};
use teamwire_store::{NewCompletion, Store};

use rig::{Rig, role, to};

struct Two {
    rig: Rig,
    beta: Store,
    db: Place,
}

/// Team alpha with `api`; team beta with an implementer `db`.
fn two_teams() -> Two {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    let places = rig.add_team("beta", "lead-2", &[("db", MemberKind::Implementer)]);
    let beta = rig.store_of("beta");
    let db = places.into_iter().find(|(n, _)| n == "db").unwrap().1;
    Two { rig, beta, db }
}

fn generation(store: &Store) -> u32 {
    store
        .read(|tx| tx.member(&role("db")))
        .unwrap()
        .unwrap()
        .generation
}

/// beta's `db` records a result with `summary`, and its turn ends.
fn finish(t: &mut Two, summary: &str) {
    let now = t.rig.now();
    let generation = generation(&t.beta);
    t.beta
        .write(|tx| {
            let c = tx.insert_completion(
                now,
                &NewCompletion {
                    role: role("db"),
                    generation,
                    status: CompletionStatus::Done,
                    summary: summary.to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )?;
            tx.deliver_completion_via_watch(now, &c.ulid).map(|_| ())
        })
        .unwrap();
}

fn woken(store: &Store) -> Vec<teamwire_core::Event> {
    store
        .read(|tx| tx.events_of_type(EventType::MemberWoken)?.all_or_corrupt())
        .unwrap()
}

fn finding_to_db(t: &mut Two) -> teamwire_msg::SendReport {
    let req = t.rig.request(
        "api",
        &["db@beta"],
        MessageType::Finding,
        "the users table moved to schema v2",
    );
    send(&mut t.rig.ctx(), req).unwrap()
}

#[test]
fn a_message_from_another_team_wakes_a_finished_member() {
    let mut t = two_teams();
    finish(&mut t, "done");
    let before = generation(&t.beta);
    let report = finding_to_db(&mut t);
    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
    assert_eq!(generation(&t.beta), before + 1);
    let events = woken(&t.beta);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0].data.get("message"),
        Some(&serde_json::json!("m001"))
    );
    assert_eq!(t.rig.host.submitted(&t.db).len(), 1);
}

#[test]
fn a_message_from_another_team_is_held_for_a_handoff() {
    let mut t = two_teams();
    finish(&mut t, "handoff: next is T020");
    let before = generation(&t.beta);
    let report = finding_to_db(&mut t);
    assert!(t.rig.host.submitted(&t.db).is_empty(), "nothing is typed");
    assert_eq!(generation(&t.beta), before, "a held message wakes nothing");
    let row = t
        .beta
        .read(|tx| tx.delivery(&report.id, &to("db")))
        .unwrap()
        .unwrap();
    assert_eq!(row.delivery.state, DeliveryState::Undelivered);
    assert_eq!(
        row.delivery.error.map(|e| e.code),
        Some(DeliveryErrorCode::HeldForHandoff)
    );
    // The sender's row mirrors the hold.
    let only = &report.deliveries[0];
    assert_eq!(only.state, RecipientState::Undelivered);
    assert_eq!(only.reason, Some(DeliveryErrorCode::HeldForHandoff));
}
