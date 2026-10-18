//! `inbox`, `ack` and `log`: a member reads its messages once, a hash that
//! no longer matches is reported, and nothing is retried for a message that
//! was handed off but never read.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use std::time::Duration;

use serde_json::json;
use teamwire_core::{Actor, DeliveryState, EventType, MemberKind, MessageType};
use teamwire_msg::{
    AckOutcome, LogFilter, RecipientState, SendError, ack, inbox, log, resend, send,
};

use rig::{Rig, id, role, to};

fn team_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    rig.add_member("web", MemberKind::Implementer);
    rig.add_member("scout", MemberKind::Investigator);
    rig
}

fn say(
    rig: &mut Rig,
    from: &str,
    target: &str,
    kind: MessageType,
    text: &str,
) -> teamwire_msg::SendReport {
    let req = rig.request(from, &[target], kind, text);
    send(&mut rig.ctx(), req).unwrap()
}

fn state_of(rig: &Rig, message: &teamwire_msg::SendReport, who: &str) -> DeliveryState {
    rig.store
        .read(|tx| tx.delivery(&message.id, &to(who)))
        .unwrap()
        .unwrap()
        .delivery
        .state
}

#[test]
fn inbox_returns_unread_messages_oldest_first_and_marks_them_read() {
    let mut rig = team_rig();
    say(&mut rig, "scout", "api", MessageType::Finding, "first");
    say(&mut rig, "scout", "api", MessageType::Finding, "second");
    say(&mut rig, "scout", "web", MessageType::Finding, "for web");
    let items = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(texts, ["first", "second"]);
    let first = items.first().unwrap();
    assert_eq!(first.display_id, id("m001"));
    assert_eq!(first.from, Actor::parse("scout").unwrap());
    assert_eq!(first.kind, MessageType::Finding);
    assert!(first.hash_ok);
    // Each message is returned once.
    assert!(
        inbox(&mut rig.ctx(), &role("api"), false)
            .unwrap()
            .is_empty()
    );
    // Another member's message is not mine.
    assert_eq!(inbox(&mut rig.ctx(), &role("web"), false).unwrap().len(), 1);
    assert_eq!(rig.events(EventType::DeliveryAcked).len(), 3);
}

#[test]
fn inbox_all_shows_read_messages_and_marks_nothing() {
    let mut rig = team_rig();
    let a = say(&mut rig, "scout", "api", MessageType::Finding, "one");
    say(&mut rig, "scout", "api", MessageType::Finding, "two");
    ack(
        &mut rig.ctx(),
        &role("api"),
        std::slice::from_ref(&a.display_id),
    )
    .unwrap();
    let all = inbox(&mut rig.ctx(), &role("api"), true).unwrap();
    assert_eq!(all.len(), 2);
    // Nothing was marked by looking: the second is still unread.
    let unread = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread.first().unwrap().text, "two");
}

#[test]
fn an_answer_shows_the_question_it_answers() {
    let mut rig = team_rig();
    let q = say(
        &mut rig,
        "api",
        "scout",
        MessageType::Question,
        "which shape?",
    );
    let mut answer = rig.request("scout", &["api"], MessageType::Answer, "the flat one");
    answer.re = Some(q.display_id.clone());
    send(&mut rig.ctx(), answer).unwrap();
    let items = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    assert_eq!(items.first().unwrap().re, Some(q.display_id));
}

#[test]
fn hash_mismatch_reported() {
    let mut rig = team_rig();
    say(
        &mut rig,
        "scout",
        "api",
        MessageType::Finding,
        "the true text",
    );
    // The body changes on disk after the send.
    rig.store
        .write(|tx| {
            tx.connection().execute(
                "UPDATE messages SET body = 'a changed text' WHERE display_id = 'm001'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let items = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    let item = items.first().unwrap();
    assert!(!item.hash_ok);
    assert_eq!(item.text, "a changed text");
    let errors = rig.events(EventType::StoreError);
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors.first().unwrap().data.get("code"),
        Some(&json!("hash_mismatch"))
    );
    // It is not marked read, so the member is told again. A healthy message
    // next to it is returned with its flag true, and is marked read.
    say(
        &mut rig,
        "scout",
        "api",
        MessageType::Finding,
        "another text",
    );
    let next = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    let flags: Vec<bool> = next.iter().map(|i| i.hash_ok).collect();
    assert_eq!(flags, [false, true]);
    let last = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    assert_eq!(last.len(), 1);
    assert!(!last.first().unwrap().hash_ok);
}

#[test]
fn unacked_stays_handed_off_no_retry() {
    let mut rig = team_rig();
    let sent = say(
        &mut rig,
        "scout",
        "api",
        MessageType::Finding,
        "please read",
    );
    assert_eq!(state_of(&rig, &sent, "api"), DeliveryState::HandedOff);
    // The member never calls inbox. Hours pass.
    rig.clock.advance(Duration::from_secs(4 * 3600));
    assert_eq!(state_of(&rig, &sent, "api"), DeliveryState::HandedOff);
    // Nothing typed the line again, and nothing may resend a reached message.
    assert_eq!(rig.host.log().count_named("submit"), 1);
    let again = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None);
    assert!(matches!(again, Err(SendError::NotResendable(_))));
    assert_eq!(rig.host.log().count_named("submit"), 1);
    // The log says so: handed off, not read.
    let rows = log(
        &rig.ctx(),
        LogFilter {
            viewer: Actor::Lead,
            role: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(
        rows.first().unwrap().deliveries.first().unwrap().state,
        RecipientState::HandedOff
    );
}

#[test]
fn ack_reports_acked_already_and_unknown() {
    let mut rig = team_rig();
    let mine = say(&mut rig, "scout", "api", MessageType::Finding, "mine");
    let theirs = say(&mut rig, "scout", "web", MessageType::Finding, "theirs");
    let ids = [
        mine.display_id.clone(),
        theirs.display_id.clone(),
        id("m099"),
    ];
    let first = ack(&mut rig.ctx(), &role("api"), &ids).unwrap();
    let outcomes: Vec<AckOutcome> = first.iter().map(|r| r.outcome).collect();
    // A message sent to someone else is unknown to me, not read.
    assert_eq!(
        outcomes,
        [AckOutcome::Acked, AckOutcome::Unknown, AckOutcome::Unknown]
    );
    assert_eq!(state_of(&rig, &theirs, "web"), DeliveryState::HandedOff);
    let second = ack(&mut rig.ctx(), &role("api"), &ids[..1]).unwrap();
    assert_eq!(second.first().unwrap().outcome, AckOutcome::Already);
    // Reading after acking returns nothing new.
    assert!(
        inbox(&mut rig.ctx(), &role("api"), false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn log_shows_what_the_viewer_may_see() {
    let mut rig = team_rig();
    say(&mut rig, "scout", "api", MessageType::Finding, "to api");
    say(&mut rig, "scout", "web", MessageType::Finding, "to web");
    say(&mut rig, "lead", "web", MessageType::Note, "from lead");
    let filter = |viewer: &str, role: Option<&str>, limit| LogFilter {
        viewer: Actor::parse(viewer).unwrap(),
        role: role.map(|r| Actor::parse(r).unwrap()),
        limit,
    };
    let ids = |rows: Vec<teamwire_msg::LogRow>| -> Vec<String> {
        rows.into_iter().map(|r| r.display_id.to_string()).collect()
    };
    // The lead sees everything, a member only what it sent or received.
    assert_eq!(
        ids(log(&rig.ctx(), filter("lead", None, None)).unwrap()),
        ["m001", "m002", "m003"]
    );
    assert_eq!(
        ids(log(&rig.ctx(), filter("api", None, None)).unwrap()),
        ["m001"]
    );
    assert_eq!(
        ids(log(&rig.ctx(), filter("scout", None, None)).unwrap()),
        ["m001", "m002"]
    );
    // `role` narrows both views.
    assert_eq!(
        ids(log(&rig.ctx(), filter("lead", Some("web"), None)).unwrap()),
        ["m002", "m003"]
    );
    assert_eq!(
        ids(log(&rig.ctx(), filter("scout", Some("api"), None)).unwrap()),
        ["m001"]
    );
    assert_eq!(
        ids(log(&rig.ctx(), filter("web", Some("api"), None)).unwrap()),
        Vec::<String>::new()
    );
    // `limit` keeps the newest.
    assert_eq!(
        ids(log(&rig.ctx(), filter("lead", None, Some(2))).unwrap()),
        ["m002", "m003"]
    );
}

#[test]
fn a_log_row_carries_a_sanitised_excerpt_and_each_recipients_state() {
    let mut rig = team_rig();
    rig.set_reachable("web", false);
    let body = format!("line one\nESC \u{1b}[31m red {}", "x".repeat(400));
    let req = rig.request("scout", &["api", "web"], MessageType::Finding, &body);
    send(&mut rig.ctx(), req).unwrap();
    let rows = log(
        &rig.ctx(),
        LogFilter {
            viewer: Actor::Lead,
            role: None,
            limit: None,
        },
    )
    .unwrap();
    let row = rows.first().unwrap();
    assert!(row.excerpt.chars().count() <= 300);
    assert!(!row.excerpt.contains('\n') && !row.excerpt.contains('\u{1b}'));
    assert!(row.excerpt.starts_with("line one ESC"));
    let states: Vec<(String, RecipientState)> = row
        .deliveries
        .iter()
        .map(|d| (d.to.to_string(), d.state))
        .collect();
    assert_eq!(
        states,
        [
            ("api".to_owned(), RecipientState::HandedOff),
            ("web".to_owned(), RecipientState::Undelivered)
        ]
    );
    assert_eq!(row.to, [to("api"), to("web")]);
}

fn damage_body(rig: &mut Rig, display_id: &str) {
    let sql = format!("UPDATE messages SET body = 'changed' WHERE display_id = '{display_id}'");
    rig.store
        .write(|tx| {
            tx.connection().execute(&sql, [])?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_damaged_question_does_not_stop_the_inbox_of_its_answer() {
    let mut rig = team_rig();
    let q = say(
        &mut rig,
        "api",
        "scout",
        MessageType::Question,
        "which shape?",
    );
    let mut answer = rig.request("scout", &["api"], MessageType::Answer, "the flat one");
    answer.re = Some(q.display_id.clone());
    send(&mut rig.ctx(), answer).unwrap();
    damage_body(&mut rig, q.display_id.as_str());
    let items = inbox(&mut rig.ctx(), &role("api"), false).unwrap();
    assert_eq!(items.first().unwrap().text, "the flat one");
}

#[test]
fn a_damaged_message_does_not_stop_ack_of_the_others() {
    let mut rig = team_rig();
    let bad = say(&mut rig, "scout", "api", MessageType::Finding, "one");
    let good = say(&mut rig, "scout", "api", MessageType::Finding, "two");
    damage_body(&mut rig, bad.display_id.as_str());
    let ids = [bad.display_id.clone(), good.display_id.clone()];
    let results = ack(&mut rig.ctx(), &role("api"), &ids).unwrap();
    let outcomes: Vec<AckOutcome> = results.iter().map(|r| r.outcome).collect();
    assert_eq!(outcomes, [AckOutcome::Unknown, AckOutcome::Acked]);
    assert_eq!(rig.events(EventType::StoreError).len(), 1);
}
