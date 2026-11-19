//! A reprompt starts the member's next generation. A delivery that is known
//! to have failed gives the generation back; one that may have arrived keeps
//! it (INV-5).
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use std::time::Duration;

use teamwire_core::{
    Actor, EventType, HostError, HostErrorCode, MemberKind, MessageType, Pid, Refusal, Timestamp,
};
use teamwire_fakes::Fault;
use teamwire_msg::{RecipientState, SendError, resend, send, watch_running};
use teamwire_store::NewMessage;

use rig::{Rig, role};

fn team_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    rig.add_member("web", MemberKind::Implementer);
    rig
}

fn generation(rig: &Rig, name: &str) -> (u32, Timestamp) {
    let m = rig
        .store
        .read(|tx| tx.member(&role(name)))
        .unwrap()
        .unwrap();
    (m.generation, m.gen_started_at)
}

fn reprompt(rig: &Rig, targets: &[&str]) -> teamwire_core::SendRequest {
    rig.request(
        "lead",
        targets,
        MessageType::Reprompt,
        "fix the two findings",
    )
}

fn go(rig: &mut Rig, targets: &[&str]) -> Result<teamwire_msg::SendReport, SendError> {
    let req = reprompt(rig, targets);
    send(&mut rig.ctx(), req)
}

#[test]
fn a_delivered_reprompt_starts_the_next_generation() {
    let mut rig = team_rig();
    let (before, started) = generation(&rig, "api");
    rig.clock.advance(Duration::from_secs(30));
    let report = go(&mut rig, &["api"]).unwrap();
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::HandedOff
    );
    let (after, after_started) = generation(&rig, "api");
    assert_eq!(after, before + 1);
    assert!(after_started > started);
    // The other member is untouched.
    assert_eq!(generation(&rig, "web").0, before);
    let events = rig.events(EventType::MemberReprompted);
    assert_eq!(events.len(), 1);
    assert_eq!(events.first().unwrap().role, Some(role("api")));
}

#[test]
fn a_reprompt_known_to_have_failed_gives_the_generation_back() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let before = generation(&rig, "api");
    let report = go(&mut rig, &["api"]).unwrap();
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::Undelivered
    );
    assert_eq!(generation(&rig, "api"), before);
    // The bump and its undo are both on record.
    let events = rig.events(EventType::MemberReprompted);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events.last().unwrap().data.get("undone"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn unconfirmed_reprompt_keeps_generation() {
    let mut rig = team_rig();
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Stalled, "stalled")),
        1,
    );
    let (before, _) = generation(&rig, "api");
    let report = go(&mut rig, &["api"]).unwrap();
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::Unconfirmed
    );
    assert_eq!(generation(&rig, "api").0, before + 1);
    assert_eq!(rig.events(EventType::MemberReprompted).len(), 1);
    // An explicit resend that fails cleanly still keeps it.
    rig.set_reachable("api", false);
    resend(&mut rig.ctx(), &Actor::Lead, &report.display_id, None).unwrap();
    assert_eq!(generation(&rig, "api").0, before + 1);
}

#[test]
fn only_the_failed_member_of_several_gives_its_generation_back() {
    let mut rig = team_rig();
    rig.set_reachable("web", false);
    let (api_before, _) = generation(&rig, "api");
    let (web_before, _) = generation(&rig, "web");
    go(&mut rig, &["api", "web"]).unwrap();
    assert_eq!(generation(&rig, "api").0, api_before + 1);
    assert_eq!(generation(&rig, "web").0, web_before);
}

#[test]
fn resuming_a_failed_reprompt_starts_the_generation_again() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let (before, _) = generation(&rig, "api");
    let mut req = reprompt(&rig, &["api"]);
    req.client_key = Some("k".to_owned());
    send(&mut rig.ctx(), req.clone()).unwrap();
    assert_eq!(generation(&rig, "api").0, before);
    // The member is reachable again; the same key delivers and bumps once.
    rig.set_reachable("api", true);
    let report = send(&mut rig.ctx(), req.clone()).unwrap();
    assert!(report.duplicate_of_key);
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::HandedOff
    );
    assert_eq!(generation(&rig, "api").0, before + 1);
    // A third call with the key changes nothing: the row is handed off.
    send(&mut rig.ctx(), req).unwrap();
    assert_eq!(generation(&rig, "api").0, before + 1);
}

#[test]
fn a_resend_of_a_failed_reprompt_starts_the_generation_again() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let (before, _) = generation(&rig, "api");
    let report = go(&mut rig, &["api"]).unwrap();
    rig.set_reachable("api", true);
    resend(&mut rig.ctx(), &Actor::Lead, &report.display_id, None).unwrap();
    assert_eq!(generation(&rig, "api").0, before + 1);
}

#[test]
fn a_reprompt_is_refused_while_the_team_closes() {
    let mut rig = team_rig();
    let now = rig.now();
    rig.store
        .write(|tx| tx.begin_close(now, Pid::new(4242)).map(|_| ()))
        .unwrap();
    let before = generation(&rig, "api");
    let err = go(&mut rig, &["api"]).unwrap_err();
    assert_eq!(err, SendError::Refused(Refusal::TeamClosed));
    assert_eq!(generation(&rig, "api"), before);
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn a_member_that_never_started_cannot_be_reprompted() {
    let mut rig = team_rig();
    let at = rig.now();
    rig.store
        .write(|tx| {
            tx.remove_member(at, &role("web"), None, &serde_json::json!({}))
                .map(|_| ())
        })
        .unwrap();
    let err = go(&mut rig, &["web"]).unwrap_err();
    assert_eq!(err, SendError::Refused(Refusal::RemovedRecipient));
}

#[test]
fn a_reprompt_line_is_marked_as_the_leads() {
    let mut rig = team_rig();
    go(&mut rig, &["api"]).unwrap();
    let typed = rig
        .host
        .log()
        .calls()
        .into_iter()
        .find(|c| c.name == "submit")
        .unwrap()
        .detail;
    assert!(typed.contains("msg m001 reprompt from lead]"), "{typed}");
    assert!(typed.contains("This message is from the lead."), "{typed}");
}

#[test]
fn watch_running_reports_a_live_lease_holder_only() {
    let mut rig = team_rig();
    assert_eq!(watch_running(&rig.ctx()).unwrap(), None);
    let now = rig.now();
    rig.store
        .write(|tx| {
            tx.take_watch_lease(now, Pid::new(777), "host", |_| true)
                .map(|_| ())
        })
        .unwrap();
    // The lease names a pid, but nothing runs it.
    assert_eq!(watch_running(&rig.ctx()).unwrap(), None);
    rig.platform.set_alive(Pid::new(777), true);
    assert_eq!(watch_running(&rig.ctx()).unwrap(), Some(Pid::new(777)));
}

/// A reprompt whose sender died after the commit: the row is `pending` and
/// the member's generation is already bumped. Returns the stored message's
/// display id.
fn crashed_after_the_bump(rig: &mut Rig, key: &str) -> teamwire_core::DisplayId {
    let at = rig.now();
    let new = NewMessage {
        sender: Actor::Lead,
        kind: MessageType::Reprompt,
        re: None,
        commit: None,
        body: "fix the two findings".to_owned(),
        client_key: Some(key.to_owned()),
        recipients: vec![teamwire_core::Recipient::Member(role("api"))],
    };
    rig.store
        .write(|tx| {
            let stored = tx.insert_message(at, &new)?;
            tx.bump_generation(at, &role("api"), stored.message.display_id.as_str())?;
            Ok(stored.message.display_id)
        })
        .unwrap()
}

#[test]
fn a_resumed_pending_reprompt_that_fails_gives_the_generation_back() {
    let mut rig = team_rig();
    let before = generation(&rig, "api");
    rig.clock.advance(Duration::from_secs(5));
    crashed_after_the_bump(&mut rig, "k");
    assert_eq!(generation(&rig, "api").0, before.0 + 1);
    // Over a minute later, so the dead sender's attempt is no longer live.
    rig.clock.advance(Duration::from_secs(61));
    rig.set_reachable("api", false);
    let mut req = reprompt(&rig, &["api"]);
    req.client_key = Some("k".to_owned());
    let report = send(&mut rig.ctx(), req).unwrap();
    assert!(report.duplicate_of_key);
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::Undelivered
    );
    assert_eq!(generation(&rig, "api"), before);
}

/// Reprompt, handoff, reprompt: the generations are 2, 3 and 4, and giving
/// the last one back returns to the handoff's generation and start time.
#[test]
fn a_reprompt_after_a_handoff_gets_the_next_generation_and_gives_back_to_the_handoff() {
    let mut rig = Rig::new();
    let place = rig.add_member("api", MemberKind::Implementer);
    rig.clock.advance(Duration::from_secs(5));
    go(&mut rig, &["api"]).unwrap();
    assert_eq!(generation(&rig, "api").0, 2);

    rig.clock.advance(Duration::from_secs(5));
    let at = rig.now();
    rig.store
        .write(|tx| {
            let session = teamwire_core::SessionId::parse("api-new").unwrap();
            tx.record_handoff(at, &role("api"), &session, &place)
        })
        .unwrap();
    assert_eq!(generation(&rig, "api"), (3, at));

    rig.clock.advance(Duration::from_secs(5));
    crashed_after_the_bump(&mut rig, "k");
    assert_eq!(generation(&rig, "api").0, 4);

    // The dead sender's attempt is no longer live; the resume fails cleanly.
    rig.clock.advance(Duration::from_secs(61));
    rig.set_reachable("api", false);
    let mut req = reprompt(&rig, &["api"]);
    req.client_key = Some("k".to_owned());
    send(&mut rig.ctx(), req).unwrap();
    assert_eq!(generation(&rig, "api"), (3, at));
}

#[test]
fn a_resent_pending_reprompt_that_fails_gives_the_generation_back() {
    let mut rig = team_rig();
    let before = generation(&rig, "api");
    rig.clock.advance(Duration::from_secs(5));
    let display = crashed_after_the_bump(&mut rig, "k");
    rig.clock.advance(Duration::from_secs(61));
    rig.set_reachable("api", false);
    let report = resend(&mut rig.ctx(), &Actor::Lead, &display, None).unwrap();
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::Undelivered
    );
    assert_eq!(generation(&rig, "api"), before);
}

#[test]
fn an_unrelated_damaged_event_does_not_stop_the_undo_of_a_resumed_reprompt() {
    let mut rig = team_rig();
    let before = generation(&rig, "api");
    rig.clock.advance(Duration::from_secs(5));
    crashed_after_the_bump(&mut rig, "k");
    let at = rig.now();
    rig.store
        .write(|tx| {
            tx.record_event(
                at,
                EventType::WatchLine,
                Some(&role("api")),
                None,
                &serde_json::json!({}),
            )?;
            tx.connection()
                .execute(
                    "UPDATE events SET time = 'yesterday' WHERE type = 'watch.line'",
                    [],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    rig.clock.advance(Duration::from_secs(61));
    rig.set_reachable("api", false);
    let mut req = reprompt(&rig, &["api"]);
    req.client_key = Some("k".to_owned());
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(
        report.deliveries.first().unwrap().state,
        RecipientState::Undelivered
    );
    assert_eq!(generation(&rig, "api"), before);
}
