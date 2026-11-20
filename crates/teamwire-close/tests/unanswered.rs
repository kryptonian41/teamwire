//! ADR 0059: a whole-team close checks each member's newest reprompt from
//! the lead on its own. A reprompt that was refused, or whose generation was
//! given back, stays unanswered for that member until the lead reprompts
//! that same member again and the new generation starts. A note, a finding,
//! or a reprompt to another member does not answer it, and neither does an
//! `--expect` that names the member's older result. Only `--abandon` closes
//! such a team as it is.

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, TEAM};
use serde_json::json;
use teamwire_close::{CloseOpts, CloseTarget, Expected, close, close_observed, plan_close_with};
use teamwire_core::{
    Actor, Clock as _, CompletionStatus, EventType, FinalSource, MemberKind, MessageType,
    Recipient, Role, TeamName, TeamState,
};
use teamwire_store::{NewCompletion, NewMessage, Store};

fn role(name: &str) -> Role {
    Role::parse(name).unwrap_or_else(|e| unreachable!("{name} is a role: {e}"))
}

/// `api` done in generation 2 and `web` done in generation 1, each
/// generation after the first started by a stored lead reprompt.
fn mixed() -> Rig {
    let mut rig = Rig::new();
    rig.add_pane_member("api", MemberKind::Implementer);
    rig.add_pane_member("web", MemberKind::Implementer);
    reprompt(&mut rig, "api", false);
    result(&mut rig, "api", 2, CompletionStatus::Done);
    result(&mut rig, "web", 1, CompletionStatus::Done);
    rig
}

/// Stores a lead message of `kind` to `to` and returns its display id.
fn store_message(rig: &mut Rig, kind: MessageType, to: &str) -> String {
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            let stored = tx.insert_message(
                now,
                &NewMessage {
                    sender: Actor::Lead,
                    kind,
                    re: None,
                    commit: None,
                    body: "more work".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Member(role(to))],
                },
            )?;
            Ok(stored.message.display_id.to_string())
        })
        .unwrap_or_else(|e| unreachable!("the message is stored: {e}"))
}

/// A lead reprompt to `to` that starts its next generation, as a send does.
/// With `undone`, its delivery failed and the generation was given back.
fn reprompt(rig: &mut Rig, to: &str, undone: bool) -> String {
    let id = store_message(rig, MessageType::Reprompt, to);
    let now = rig.clock.now();
    let who = role(to);
    rig.store
        .write(|tx| {
            let before = tx.reader().member(&who)?;
            let (generation, started_at) =
                before.map_or((1, now), |m| (m.generation, m.gen_started_at));
            tx.bump_generation(now, &who, &id)?;
            if undone {
                tx.restore_generation(
                    now,
                    &who,
                    generation,
                    started_at,
                    EventType::MemberReprompted,
                )?;
            }
            Ok(())
        })
        .unwrap_or_else(|e| unreachable!("the generation moves: {e}"));
    id
}

/// A lead send to `to` that the rules refused, recorded as a send records it.
fn refused(rig: &mut Rig, to: &str, kind: Option<&str>) {
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.record_event(
                now,
                EventType::MessageRefused,
                None,
                None,
                &json!({"code": "rate_limited", "reason": "rate_limited", "type": kind,
                        "sender": "lead", "to": [to]}),
            )
        })
        .unwrap_or_else(|e| unreachable!("the refusal is recorded: {e}"));
}

fn result(rig: &mut Rig, who: &str, generation: u32, status: CompletionStatus) {
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.insert_completion(
                now,
                &NewCompletion {
                    role: role(who),
                    generation,
                    status,
                    summary: "finished".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
            .map(|_| ())
        })
        .unwrap_or_else(|e| unreachable!("the result is stored: {e}"));
}

fn expecting(expect: &[&str]) -> CloseOpts {
    CloseOpts {
        expect: expect
            .iter()
            .map(|e| Expected::parse(e).unwrap_or_else(|why| unreachable!("{e}: {why}")))
            .collect(),
        ..CloseOpts::default()
    }
}

/// The dry run and the close both refuse with `said`, and nothing was
/// taken away: the team is open, its store is there, and no archive exists.
fn refuses(rig: &mut Rig, opts: &CloseOpts, said: &str) {
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, opts);
    assert!(
        plan.refusals.iter().any(|r| r.contains(said)),
        "the dry run: {plan:?}"
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts.clone());
    assert!(report.steps.is_empty(), "{report:?}");
    assert!(
        report.refusals.iter().any(|r| r.contains(said)),
        "the close: {report:?}"
    );
    assert_eq!(rig.team().state, TeamState::Open);
    assert!(rig.team_dir().join("team.db").is_file());
    assert!(!rig.archive_root().exists());
}

fn closes(rig: &mut Rig, opts: CloseOpts) {
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
    assert!(plan.refusals.is_empty(), "the dry run: {plan:?}");
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty(), "{report:?}");
    assert!(report.archive.is_some_and(|a| a.is_dir()));
    assert!(!rig.team_dir().exists());
}

const UNDONE: &str = "api's newest reprompt from the lead, m002, started no generation";

/// The case that was seen: api's reprompt for generation 3 was undone, and
/// the lead then sent a note to web. The note is the lead's newest message,
/// and it says nothing about api.
#[test]
fn a_note_to_another_member_after_an_undone_reprompt_does_not_let_the_close_through() {
    let mut rig = mixed();
    reprompt(&mut rig, "api", true);
    store_message(&mut rig, MessageType::Note, "web");
    refuses(&mut rig, &CloseOpts::default(), UNDONE);
    // An expectation of api's older result is no answer to the reprompt.
    refuses(&mut rig, &expecting(&["api:2:done"]), UNDONE);
    refuses(&mut rig, &expecting(&["api:2:done", "web:1:done"]), UNDONE);
}

/// A finding, a note to the same member, or a reprompt that started web's
/// next generation answers nothing for api.
#[test]
fn only_a_new_reprompt_to_the_same_member_answers_it() {
    let mut rig = mixed();
    reprompt(&mut rig, "api", true);
    store_message(&mut rig, MessageType::Note, "api");
    store_message(&mut rig, MessageType::Finding, "web");
    reprompt(&mut rig, "web", false);
    result(&mut rig, "web", 2, CompletionStatus::Done);
    refuses(&mut rig, &CloseOpts::default(), UNDONE);

    // The lead reprompts api again, and generation 3 starts. The close now
    // waits for api's result in that generation.
    reprompt(&mut rig, "api", false);
    refuses(
        &mut rig,
        &CloseOpts::default(),
        "api is in generation 3 with no result yet",
    );
    result(&mut rig, "api", 3, CompletionStatus::Done);
    closes(&mut rig, CloseOpts::default());
}

/// A reprompt the rules refused is unanswered the same way. A send whose
/// type was never read counts as a reprompt, since it may have been one.
#[test]
fn a_refused_reprompt_is_answered_only_by_a_new_reprompt_to_that_member() {
    for kind in [Some("reprompt"), None] {
        let mut rig = mixed();
        refused(&mut rig, "api", kind);
        store_message(&mut rig, MessageType::Note, "api");
        store_message(&mut rig, MessageType::Note, "web");
        let said = "api's newest reprompt from the lead was refused (rate_limited";
        refuses(&mut rig, &CloseOpts::default(), said);
        refuses(&mut rig, &expecting(&["api:2:done"]), said);

        reprompt(&mut rig, "api", false);
        result(&mut rig, "api", 3, CompletionStatus::Done);
        closes(&mut rig, expecting(&["api:3:done"]));
    }
}

/// A refused note asks for no new work, so it leaves no reprompt unanswered.
#[test]
fn a_refused_note_leaves_no_reprompt_unanswered() {
    let mut rig = mixed();
    refused(&mut rig, "api", Some("note"));
    store_message(&mut rig, MessageType::Note, "api");
    closes(&mut rig, CloseOpts::default());
}

/// A reprompt that started its generation and later one that was undone:
/// the newest one counts.
#[test]
fn the_newest_reprompt_to_a_member_counts() {
    let mut rig = mixed();
    reprompt(&mut rig, "api", false);
    result(&mut rig, "api", 3, CompletionStatus::Done);
    reprompt(&mut rig, "api", true);
    refuses(
        &mut rig,
        &CloseOpts::default(),
        "api's newest reprompt from the lead, m003, started no generation",
    );
}

/// `--abandon` closes the team as it is, on purpose.
#[test]
fn abandon_closes_a_team_with_an_unanswered_reprompt() {
    let mut rig = mixed();
    reprompt(&mut rig, "api", true);
    closes(
        &mut rig,
        CloseOpts {
            abandon: Some("the user stopped api's work".to_owned()),
            ..CloseOpts::default()
        },
    );
}

/// The check runs again under the close lock: a reprompt to api refused
/// after the plan stops the close with nothing removed.
#[test]
fn a_reprompt_refused_before_the_lock_stops_the_close() {
    let mut rig = mixed();
    let home = rig.home.path().to_path_buf();
    let now = rig.clock.now();
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "lock" {
                let mut store =
                    Store::open_existing(&home, &TeamName::parse(TEAM).unwrap()).unwrap();
                store
                    .write(|tx| {
                        tx.record_event(
                            now,
                            EventType::MessageRefused,
                            None,
                            None,
                            &json!({"code": "rate_limited", "type": "reprompt",
                                    "sender": "lead", "to": ["api"]}),
                        )
                    })
                    .unwrap();
            }
        },
    );
    assert_eq!(report.stopped_at.as_deref(), Some("expect"), "{report:?}");
    let failed = report.steps.last().unwrap();
    assert!(
        failed
            .detail
            .contains("api's newest reprompt from the lead was refused"),
        "{failed:?}"
    );
    assert_eq!(rig.team().state, TeamState::Open);
    assert!(!rig.archive_root().exists());
}
