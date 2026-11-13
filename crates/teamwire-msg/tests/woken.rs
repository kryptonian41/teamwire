//! A message that wakes a finished member starts a generation that the watch
//! tracks, recorded as `member.woken` (ADR 0043 decisions 8 and 9). A known
//! delivery failure gives the generation back, as for a reprompt (REP-3).
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{CompletionStatus, EventType, FinalSource, MemberKind, MessageType};
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
