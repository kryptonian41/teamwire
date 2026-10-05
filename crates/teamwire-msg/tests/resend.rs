//! `resend`: another delivery attempt on the same stored message (ADR 0003
//! point 2a). It never makes a message and skips the burst and repeat limits.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Actor, DeliveryErrorCode, DeliveryState, EventType, HostError, HostErrorCode, MemberKind,
    MessageType, Refusal,
};
use teamwire_fakes::Fault;
use teamwire_msg::{RecipientState, SendError, SendReport, resend, send};
use teamwire_store::{DeliveryUpdate, NewMessage};

use rig::{Rig, id, to};

fn team_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    rig.add_member("web", MemberKind::Implementer);
    rig.add_member("scout", MemberKind::Investigator);
    rig
}

fn state_of(report: &SendReport, name: &str) -> RecipientState {
    report
        .deliveries
        .iter()
        .find(|d| d.to == to(name))
        .unwrap()
        .state
}

fn submits(rig: &Rig) -> usize {
    rig.host.log().count_named("submit")
}

#[test]
fn resend_retries_failed_recipients_only() {
    let mut rig = team_rig();
    rig.set_reachable("web", false);
    let req = rig.request(
        "scout",
        &["api", "web"],
        MessageType::Finding,
        "the finding",
    );
    let first = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(state_of(&first, "api"), RecipientState::HandedOff);
    assert_eq!(state_of(&first, "web"), RecipientState::Undelivered);
    assert_eq!(submits(&rig), 1);

    rig.set_reachable("web", true);
    let again = resend(
        &mut rig.ctx(),
        &Actor::parse("scout").unwrap(),
        &first.display_id,
        None,
    )
    .unwrap();
    // The same message, a second attempt; the reached recipient is skipped.
    assert_eq!(again.id, first.id);
    assert_eq!(again.display_id, first.display_id);
    assert_eq!(state_of(&again, "api"), RecipientState::Skipped);
    assert_eq!(state_of(&again, "web"), RecipientState::HandedOff);
    let web = again.deliveries.iter().find(|d| d.to == to("web")).unwrap();
    assert_eq!(web.attempts, Some(2));
    assert_eq!(submits(&rig), 2);
    // No second message was made, and the attempt is on record.
    assert_eq!(
        rig.store.read(|tx| tx.counter("display_id")).unwrap(),
        Some(1)
    );
    assert_eq!(rig.events(EventType::DeliveryResent).len(), 1);
    assert_eq!(rig.events(EventType::MessageSent).len(), 1);
}

#[test]
fn repeat_check_ignores_undelivered_copy() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let req = rig.request("scout", &["api"], MessageType::Finding, "the finding");
    let first = send(&mut rig.ctx(), req.clone()).unwrap();
    assert_eq!(state_of(&first, "api"), RecipientState::Undelivered);
    // The sender tries again with the same words: not refused as a repeat.
    rig.set_reachable("api", true);
    let second = send(&mut rig.ctx(), req.clone()).unwrap();
    assert_eq!(state_of(&second, "api"), RecipientState::HandedOff);
    assert_ne!(second.id, first.id);
    // Once a copy was handed off, the same words are a repeat.
    assert_eq!(
        send(&mut rig.ctx(), req).unwrap_err(),
        SendError::Refused(Refusal::Repeat)
    );
}

#[test]
fn only_sender_or_lead_may_resend() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let sent = send(&mut rig.ctx(), req).unwrap();
    rig.set_reachable("api", true);
    let before = rig
        .store
        .read(|tx| tx.delivery(&sent.id, &to("api")))
        .unwrap()
        .unwrap();

    let by_web = resend(
        &mut rig.ctx(),
        &Actor::parse("web").unwrap(),
        &sent.display_id,
        None,
    );
    assert_eq!(
        by_web.unwrap_err(),
        SendError::Refused(Refusal::NotResendable)
    );
    let by_recipient = resend(
        &mut rig.ctx(),
        &Actor::parse("api").unwrap(),
        &sent.display_id,
        None,
    );
    assert_eq!(
        by_recipient.unwrap_err(),
        SendError::Refused(Refusal::NotResendable)
    );
    // Nothing changed: same row, no attempt, nothing typed.
    let after = rig
        .store
        .read(|tx| tx.delivery(&sent.id, &to("api")))
        .unwrap()
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(submits(&rig), 0);
    assert!(rig.events(EventType::DeliveryResent).is_empty());

    // The lead may.
    let by_lead = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None).unwrap();
    assert_eq!(state_of(&by_lead, "api"), RecipientState::HandedOff);
}

#[test]
fn an_unconfirmed_delivery_is_resent_only_by_an_explicit_resend() {
    let mut rig = team_rig();
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Timeout, "timed out")),
        1,
    );
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let sent = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(state_of(&sent, "api"), RecipientState::Unconfirmed);
    // Time passes and nothing types it again (INV-5).
    rig.clock.advance(Duration::from_secs(3600));
    assert_eq!(submits(&rig), 1);
    let again = resend(
        &mut rig.ctx(),
        &Actor::parse("scout").unwrap(),
        &sent.display_id,
        None,
    )
    .unwrap();
    assert_eq!(state_of(&again, "api"), RecipientState::HandedOff);
    assert_eq!(submits(&rig), 2);
}

#[test]
fn a_clean_failure_of_a_resend_keeps_an_unconfirmed_delivery_unconfirmed() {
    let mut rig = team_rig();
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Stalled, "stalled")),
        1,
    );
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let sent = send(&mut rig.ctx(), req).unwrap();
    // The retry fails cleanly, but the first try may have typed the line.
    rig.set_reachable("api", false);
    let again = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None).unwrap();
    assert_eq!(state_of(&again, "api"), RecipientState::Unconfirmed);
    let reason = again.deliveries.first().unwrap().reason;
    assert_eq!(reason, Some(DeliveryErrorCode::NoLiveProcess));
    assert_eq!(again.deliveries.first().unwrap().attempts, Some(2));
}

#[test]
fn a_pending_row_is_resendable_only_after_sixty_seconds() {
    let mut rig = team_rig();
    // A crash after the commit left one pending row.
    let at = rig.now();
    let new = NewMessage {
        sender: Actor::parse("scout").unwrap(),
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: "left pending".to_owned(),
        client_key: None,
        recipients: vec![to("api")],
    };
    let stored = rig.store.write(|tx| tx.insert_message(at, &new)).unwrap();
    let display = stored.message.display_id.clone();
    let scout = Actor::parse("scout").unwrap();
    rig.clock.advance(Duration::from_secs(59));
    assert!(matches!(
        resend(&mut rig.ctx(), &scout, &display, None),
        Err(SendError::NotResendable(_))
    ));
    assert_eq!(submits(&rig), 0);
    rig.clock.advance(Duration::from_secs(2));
    let report = resend(&mut rig.ctx(), &scout, &display, None).unwrap();
    assert_eq!(state_of(&report, "api"), RecipientState::HandedOff);
}

#[test]
fn a_reached_message_has_nothing_to_resend() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let sent = send(&mut rig.ctx(), req).unwrap();
    let result = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None);
    assert!(matches!(result, Err(SendError::NotResendable(_))));
    // A read message is final too.
    let inbox = teamwire_msg::inbox(&mut rig.ctx(), &rig::role("api"), false).unwrap();
    assert_eq!(inbox.len(), 1);
    let result = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None);
    assert!(matches!(result, Err(SendError::NotResendable(_))));
}

#[test]
fn a_resend_may_name_recipients_and_skips_the_burst_limit() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    rig.set_reachable("web", false);
    let mut first = None;
    for n in 0..5 {
        let req = rig.request(
            "scout",
            &["api", "web"],
            MessageType::Finding,
            &format!("f{n}"),
        );
        let report = send(&mut rig.ctx(), req).unwrap();
        first.get_or_insert(report);
    }
    let first = first.unwrap();
    // A sixth new message would be refused, but a resend is not a new message.
    let req = rig.request("scout", &["api"], MessageType::Finding, "f5");
    assert_eq!(
        send(&mut rig.ctx(), req).unwrap_err(),
        SendError::Refused(Refusal::RateLimited)
    );
    rig.set_reachable("api", true);
    let scout = Actor::parse("scout").unwrap();
    let only_api = resend(
        &mut rig.ctx(),
        &scout,
        &first.display_id,
        Some(&[to("api")]),
    )
    .unwrap();
    assert_eq!(only_api.deliveries.len(), 1);
    assert_eq!(state_of(&only_api, "api"), RecipientState::HandedOff);
    // web was not named, so it was not touched.
    let web = rig
        .store
        .read(|tx| tx.delivery(&first.id, &to("web")))
        .unwrap()
        .unwrap();
    assert_eq!(web.delivery.state, DeliveryState::Undelivered);
    assert_eq!(web.delivery.attempts, 1);
    // A role that was never a recipient is a mistake, not a skip.
    let stranger = resend(
        &mut rig.ctx(),
        &scout,
        &first.display_id,
        Some(&[to("scout")]),
    );
    assert!(matches!(stranger, Err(SendError::InvalidArgument(_))));
    let unknown = resend(&mut rig.ctx(), &scout, &id("m099"), None);
    assert!(matches!(unknown, Err(SendError::InvalidArgument(_))));
}

#[test]
fn a_resend_to_a_removed_recipient_is_undelivered_with_its_reason() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let sent = send(&mut rig.ctx(), req).unwrap();
    let at = rig.now();
    rig.store
        .write(|tx| {
            tx.remove_member(at, &rig::role("api"), None, &json!({}))
                .map(|_| ())
        })
        .unwrap();
    let again = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None).unwrap();
    assert_eq!(state_of(&again, "api"), RecipientState::Undelivered);
    assert_eq!(
        again.deliveries.first().unwrap().reason,
        Some(DeliveryErrorCode::RecipientRemoved)
    );
}

#[test]
fn delivery_updates_keep_forward_only_moves_in_the_store_too() {
    // The store refuses a backward move, whatever the service asks.
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let sent = send(&mut rig.ctx(), req).unwrap();
    let at = rig.now();
    let back = rig.store.write(|tx| {
        tx.set_delivery_state(
            at,
            &sent.id,
            &to("api"),
            &DeliveryUpdate {
                state: DeliveryState::Pending,
                route: None,
                error: None,
                attempts: 1,
            },
        )
    });
    assert!(back.is_err());
}

#[test]
fn resend_is_refused_while_the_team_is_closing() {
    let mut rig = team_rig();
    rig.set_reachable("web", false);
    let req = rig.request("lead", &["web"], MessageType::Reprompt, "fix it");
    let first = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(state_of(&first, "web"), RecipientState::Undelivered);
    rig.set_reachable("web", true);
    let now = rig.now();
    rig.store
        .write(|tx| {
            tx.begin_close(now, teamwire_core::Pid::new(4242))
                .map(|_| ())
        })
        .unwrap();
    let refused = resend(&mut rig.ctx(), &Actor::Lead, &first.display_id, None);
    assert_eq!(
        refused.unwrap_err().to_string(),
        SendError::from(Refusal::TeamClosed).to_string()
    );
    assert_eq!(submits(&rig), 0);
    assert_eq!(rig.events(EventType::MessageRefused).len(), 1);
}

#[test]
fn a_pending_row_whose_attempt_just_started_is_not_resendable() {
    let mut rig = team_rig();
    let at = rig.now();
    let new = NewMessage {
        sender: Actor::parse("scout").unwrap(),
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: "left pending".to_owned(),
        client_key: None,
        recipients: vec![to("api")],
    };
    let stored = rig.store.write(|tx| tx.insert_message(at, &new)).unwrap();
    let display = stored.message.display_id.clone();
    let scout = Actor::parse("scout").unwrap();
    // The row is old, and a resend begins an attempt whose line is still
    // being typed: the row stays pending.
    rig.clock.advance(Duration::from_secs(600));
    let now = rig.now();
    rig.store
        .write(|tx| tx.begin_resend(now, &stored.message.ulid, &to("api")))
        .unwrap();
    rig.clock.advance(Duration::from_secs(10));
    assert!(matches!(
        resend(&mut rig.ctx(), &scout, &display, None),
        Err(SendError::NotResendable(_))
    ));
    assert_eq!(submits(&rig), 0);
    rig.clock.advance(Duration::from_secs(51));
    let report = resend(&mut rig.ctx(), &scout, &display, None).unwrap();
    assert_eq!(state_of(&report, "api"), RecipientState::HandedOff);
}
