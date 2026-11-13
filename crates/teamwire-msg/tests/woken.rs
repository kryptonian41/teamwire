//! A message that wakes a finished member starts a generation that the watch
//! tracks, recorded as `member.woken` (ADR 0043 decisions 8 and 9). A known
//! delivery failure gives the generation back, as for a reprompt (REP-3).
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{
    CompletionStatus, DeliveryErrorCode, DeliveryState, EventType, FinalSource, MemberKind,
    MessageType,
};
use teamwire_msg::send;
use teamwire_store::NewCompletion;

use rig::{Rig, id, role};

fn team_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    rig.add_member("web", MemberKind::Implementer);
    rig
}

fn generation(rig: &Rig, name: &str) -> u32 {
    rig.store
        .read(|tx| tx.member(&role(name)))
        .unwrap()
        .unwrap()
        .generation
}

/// `name` records a result with `status` and `summary` in its current
/// generation.
fn finish(rig: &mut Rig, name: &str, status: CompletionStatus, summary: &str) {
    let now = rig.now();
    let generation = generation(rig, name);
    rig.store
        .write(|tx| {
            tx.insert_completion(
                now,
                &NewCompletion {
                    role: role(name),
                    generation,
                    status,
                    summary: summary.to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
            .map(|_| ())
        })
        .unwrap();
}

fn note(rig: &mut Rig, to: &str) {
    let req = rig.request("lead", &[to], MessageType::Note, "one more thing");
    send(&mut rig.ctx(), req).unwrap();
}

#[test]
fn an_answer_to_a_member_with_a_done_result_starts_a_generation() {
    let mut rig = team_rig();
    let question = rig.request("api", &["lead"], MessageType::Question, "which base?");
    send(&mut rig.ctx(), question).unwrap();
    finish(
        &mut rig,
        "api",
        CompletionStatus::Done,
        "done; one question open",
    );
    let before = generation(&rig, "api");
    let mut answer = rig.request("lead", &["api"], MessageType::Answer, "develop");
    answer.re = Some(id("m001"));
    send(&mut rig.ctx(), answer).unwrap();
    assert_eq!(generation(&rig, "api"), before + 1);
}

#[test]
fn a_note_to_a_member_with_a_blocked_or_failed_result_starts_a_generation() {
    for status in [CompletionStatus::Blocked, CompletionStatus::Failed] {
        let mut rig = team_rig();
        finish(&mut rig, "api", status, "stuck on the base");
        let before = generation(&rig, "api");
        note(&mut rig, "api");
        assert_eq!(generation(&rig, "api"), before + 1, "{status}");
        // A member still at work is not woken: it has no result yet.
        assert_eq!(generation(&rig, "web"), before, "{status}");
    }
}

#[test]
fn the_woken_generation_is_recorded_as_member_woken() {
    let mut rig = team_rig();
    finish(&mut rig, "api", CompletionStatus::Done, "done");
    note(&mut rig, "api");
    let woken = rig.events(EventType::MemberWoken);
    assert_eq!(woken.len(), 1, "{woken:?}");
    let event = woken.first().unwrap();
    assert_eq!(event.role, Some(role("api")));
    assert_eq!(event.data.get("generation"), Some(&serde_json::json!(2)));
    assert_eq!(event.data.get("message"), Some(&serde_json::json!("m001")));
    assert!(rig.events(EventType::MemberReprompted).is_empty());
}

/// Guard: the base never bumps, so the first assert holds there too. It
/// keeps the undo once the wake exists. The second needs the fix.
#[test]
fn a_known_delivery_failure_undoes_the_woken_generation() {
    let mut rig = team_rig();
    finish(&mut rig, "api", CompletionStatus::Done, "done");
    let before = generation(&rig, "api");
    // The locator finds no process: the delivery is known to have failed.
    rig.set_reachable("api", false);
    note(&mut rig, "api");
    assert_eq!(generation(&rig, "api"), before);
    let woken = rig.events(EventType::MemberWoken);
    assert!(
        woken
            .iter()
            .any(|e| e.data.get("undone") == Some(&serde_json::json!(true))),
        "{woken:?}"
    );
}

fn submits(rig: &Rig) -> usize {
    rig.host.log().count_named("submit")
}

/// A member whose newest result asks for a handoff gets its messages held
/// for the fresh session: stored, not typed (ADR 0043 decision 11).
#[test]
fn a_message_to_a_member_with_a_handoff_result_is_held_not_typed() {
    let mut rig = team_rig();
    finish(
        &mut rig,
        "api",
        CompletionStatus::Done,
        "handoff: next is T020",
    );
    let before = generation(&rig, "api");
    let typed = submits(&rig);
    let req = rig.request("lead", &["api"], MessageType::Note, "one more thing");
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(submits(&rig), typed, "nothing is typed");
    let row = rig
        .store
        .read(|tx| tx.delivery(&report.id, &rig::to("api")))
        .unwrap()
        .unwrap();
    assert_eq!(row.delivery.state, DeliveryState::Undelivered);
    assert_eq!(
        row.delivery.error.map(|e| e.code),
        Some(DeliveryErrorCode::HeldForHandoff)
    );
    assert_eq!(
        generation(&rig, "api"),
        before,
        "a held message wakes nothing"
    );
    assert!(rig.events(EventType::MemberWoken).is_empty());
}

/// A reprompt in place of the handoff types its own text, then names the
/// held messages, oldest first.
#[test]
fn a_reprompt_names_the_held_messages_after_its_text() {
    let mut rig = team_rig();
    finish(
        &mut rig,
        "api",
        CompletionStatus::Done,
        "handoff: next is T020",
    );
    note(&mut rig, "api");
    let req = rig.request("lead", &["api"], MessageType::Reprompt, "carry on here");
    send(&mut rig.ctx(), req).unwrap();
    let typed = rig
        .host
        .log()
        .calls()
        .into_iter()
        .rfind(|c| c.name == "submit")
        .unwrap()
        .detail;
    assert!(typed.contains("msg m002 reprompt from lead]"), "{typed}");
    let body = typed.find("carry on here").unwrap();
    let held = typed.find("Held for you, oldest first: m001 (note from lead)");
    assert!(held.is_some_and(|h| h > body), "{typed}");
}

/// A resume by key tries `undelivered` rows again, but a held row keeps
/// waiting for the handoff: nothing is typed.
#[test]
fn a_resume_by_key_leaves_a_held_row_held() {
    let mut rig = team_rig();
    finish(
        &mut rig,
        "api",
        CompletionStatus::Done,
        "handoff: next is T020",
    );
    let keyed = |rig: &Rig| {
        let mut req = rig.request("lead", &["api"], MessageType::Note, "one more thing");
        req.client_key = Some("note-1".to_owned());
        req
    };
    let req = keyed(&rig);
    let first = send(&mut rig.ctx(), req).unwrap();
    let typed = submits(&rig);
    let req = keyed(&rig);
    let again = send(&mut rig.ctx(), req).unwrap();
    assert!(again.duplicate_of_key);
    assert_eq!(submits(&rig), typed, "nothing is typed");
    let row = rig
        .store
        .read(|tx| tx.delivery(&first.id, &rig::to("api")))
        .unwrap()
        .unwrap();
    assert_eq!(
        row.delivery.error.map(|e| e.code),
        Some(DeliveryErrorCode::HeldForHandoff)
    );
}
