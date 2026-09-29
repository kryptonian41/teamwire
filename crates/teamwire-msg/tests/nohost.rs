//! With no host at all (DEL-17) every message is still stored, undelivered
//! ones are reported, and the recipient reads through `inbox`.
#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{DeliveryErrorCode, MemberKind, MessageType};
use teamwire_msg::{AckOutcome, RecipientState, ack, inbox, send};

use rig::{Rig, id, role, to};

#[test]
fn no_host_member_reads_through_inbox() {
    let mut rig = Rig::new();
    rig.add_member("scout", MemberKind::Investigator);
    rig.add_member_on("blind", MemberKind::Implementer, "none");
    let req = rig.request(
        "scout",
        &["blind"],
        MessageType::Finding,
        "read me through the inbox",
    );
    let report = send(&mut rig.ctx(), req).unwrap();
    let delivery = report.deliveries.first().unwrap();
    assert_eq!(delivery.state, RecipientState::Undelivered);
    assert_eq!(delivery.reason, Some(DeliveryErrorCode::NoHost));
    // Nothing was typed anywhere, and the message is still stored.
    assert_eq!(rig.host.log().count_named("submit"), 0);
    let items = inbox(&mut rig.ctx(), &role("blind"), false).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items.first().unwrap().text, "read me through the inbox");
    assert!(items.first().unwrap().hash_ok);
    // Reading acked it, the only way a delivery becomes read.
    let again = ack(&mut rig.ctx(), &role("blind"), &[id("m001")]).unwrap();
    assert_eq!(again.first().unwrap().outcome, AckOutcome::Already);
    assert_eq!(rig.events(teamwire_core::EventType::DeliveryAcked).len(), 1);
}

#[test]
fn question_to_no_host_lead_undelivered() {
    let mut rig = Rig::with_lead_host("none");
    rig.add_member("scout", MemberKind::Investigator);
    let req = rig.request("scout", &["lead"], MessageType::Question, "which branch?");
    let report = send(&mut rig.ctx(), req).unwrap();
    let delivery = report.deliveries.first().unwrap();
    assert_eq!(delivery.to, to("lead"));
    assert_eq!(delivery.state, RecipientState::Undelivered);
    assert_eq!(delivery.reason, Some(DeliveryErrorCode::NoHost));
    // The question is stored, so the lead's watch and `log` can name it.
    let stored = rig
        .store
        .read(|tx| tx.message_by_display_id(&id("m001")))
        .unwrap()
        .unwrap();
    assert_eq!(stored.body, "which branch?");
}
