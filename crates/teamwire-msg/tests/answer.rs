//! An answer with `re` settles its question's delivery to the answerer
//! (ADR 0043 decision 3): the lead that answers a question it read from its
//! file is not told about the question again.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{DeliveryState, MemberKind, MessageType, Recipient};
use teamwire_msg::send;

use rig::{Rig, id};

fn lead_row(rig: &Rig, display: &str) -> DeliveryState {
    rig.store
        .read(|tx| {
            let m = tx.message_by_display_id(&id(display))?.unwrap();
            Ok(tx
                .delivery(&m.ulid, &Recipient::Lead)?
                .unwrap()
                .delivery
                .state)
        })
        .unwrap()
}

#[test]
fn an_answer_with_re_settles_the_questions_lead_row() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    // The lead's pane cannot be reached, so the question is not typed.
    rig.set_reachable_session("lead-1", false);
    let question = rig.request("api", &["lead"], MessageType::Question, "which base?");
    send(&mut rig.ctx(), question).unwrap();
    assert_eq!(lead_row(&rig, "m001"), DeliveryState::Undelivered);

    let mut answer = rig.request("lead", &["api"], MessageType::Answer, "develop");
    answer.re = Some(id("m001"));
    send(&mut rig.ctx(), answer).unwrap();
    assert_eq!(lead_row(&rig, "m001"), DeliveryState::Acked);
}
