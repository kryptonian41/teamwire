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
use teamwire_store::NewMessage;

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

/// A message whose delivery is `unconfirmed` (the line may be on screen),
/// then a team that starts closing. The old code resent it: only an
/// `undelivered` reprompt was refused, through its generation bump.
fn closing_team_with_unconfirmed(kind: MessageType) -> (Rig, teamwire_core::DisplayId) {
    let mut rig = team_rig();
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Stalled, "stalled")),
        1,
    );
    let (sender, text) = ("lead", "fix it");
    let req = rig.request(sender, &["web"], kind, text);
    let first = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(state_of(&first, "web"), RecipientState::Unconfirmed);
    let now = rig.now();
    rig.store
        .write(|tx| {
            tx.begin_close(now, teamwire_core::Pid::new(4242))
                .map(|_| ())
        })
        .unwrap();
    (rig, first.display_id)
}

#[test]
fn resend_is_refused_while_the_team_is_closing() {
    for kind in [MessageType::Reprompt, MessageType::Note] {
        let (mut rig, display) = closing_team_with_unconfirmed(kind);
        let before = submits(&rig);
        let refused = resend(&mut rig.ctx(), &Actor::Lead, &display, None);
        assert_eq!(
            refused.unwrap_err().to_string(),
            SendError::from(Refusal::TeamClosed).to_string()
        );
        assert_eq!(submits(&rig), before, "{kind}: nothing was typed");
        assert_eq!(rig.events(EventType::MessageRefused).len(), 1);
    }
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

#[test]
fn a_team_that_starts_closing_part_way_keeps_every_report() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    rig.set_reachable("web", false);
    let req = rig.request("scout", &["api", "web"], MessageType::Finding, "x");
    let first = send(&mut rig.ctx(), req).unwrap();
    rig.set_reachable("api", true);
    rig.set_reachable("web", true);
    // The close starts as soon as the first recipient's row is touched.
    rig.store
        .write(|tx| {
            tx.connection()
                .execute_batch(
                    "CREATE TRIGGER close_after_api AFTER UPDATE ON deliveries
                     WHEN NEW.recipient = 'api'
                     BEGIN UPDATE team SET state = 'closing'; END;",
                )
                .map_err(Into::into)
        })
        .unwrap();
    let report = resend(
        &mut rig.ctx(),
        &teamwire_core::Actor::parse("scout").unwrap(),
        &first.display_id,
        None,
    )
    .unwrap();
    assert_eq!(report.deliveries.len(), 2);
    assert_eq!(state_of(&report, "api"), RecipientState::HandedOff);
    let web = report
        .deliveries
        .iter()
        .find(|d| d.to == to("web"))
        .unwrap();
    assert_eq!(web.state, RecipientState::Undelivered);
    assert_eq!(web.reason, Some(DeliveryErrorCode::TeamClosed));
    assert_eq!(submits(&rig), 1);
}

/// A message from scout to api and web, whose first send ended with api
/// `undelivered` and web in `web_state`.
fn tried_once(rig: &mut Rig, web_state: DeliveryState) -> teamwire_core::DisplayId {
    let at = rig.now();
    let new = NewMessage {
        sender: Actor::parse("scout").unwrap(),
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: "x".to_owned(),
        client_key: None,
        recipients: vec![to("api"), to("web")],
    };
    let stored = rig.store.write(|tx| tx.insert_message(at, &new)).unwrap();
    let failed = |state| teamwire_store::DeliveryUpdate {
        state,
        route: None,
        error: Some(teamwire_core::DeliveryError::new(
            DeliveryErrorCode::HostStalled,
            "x",
        )),
        attempts: 1,
    };
    rig.store
        .write(|tx| {
            let ulid = &stored.message.ulid;
            tx.set_delivery_state(at, ulid, &to("api"), &failed(DeliveryState::Undelivered))?;
            tx.set_delivery_state(at, ulid, &to("web"), &failed(web_state))
        })
        .unwrap();
    stored.message.display_id
}

fn trigger(rig: &mut Rig, sql: &str) {
    rig.store
        .write(|tx| tx.connection().execute_batch(sql).map_err(Into::into))
        .unwrap();
}

#[test]
fn a_row_another_command_reached_after_this_one_read_it_is_skipped() {
    let mut rig = team_rig();
    let display = tried_once(&mut rig, DeliveryState::Undelivered);
    // While this resend types to api, another command hands web off.
    trigger(
        &mut rig,
        "CREATE TRIGGER web_reached AFTER UPDATE ON deliveries
         WHEN NEW.recipient = 'api' AND NEW.state = 'handed_off'
         BEGIN UPDATE deliveries SET state = 'handed_off' WHERE recipient = 'web'; END;",
    );
    let report = resend(
        &mut rig.ctx(),
        &Actor::parse("scout").unwrap(),
        &display,
        None,
    )
    .unwrap();
    assert_eq!(state_of(&report, "api"), RecipientState::HandedOff);
    assert_eq!(state_of(&report, "web"), RecipientState::Skipped);
    assert_eq!(submits(&rig), 1);
}
