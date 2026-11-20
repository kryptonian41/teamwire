//! ADR 0059: the one corrective hold of a generation that a reprompt from
//! the lead started. A reminder after a tool call is not reading the
//! reprompt, and a chat answer is not a result.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a failing unwrap or index in an integration test fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/kit.rs"]
mod kit;
#[path = "../../teamwire-deliver/tests/support/members.rs"]
mod members;
#[path = "../../teamwire-deliver/tests/support/rig.rs"]
mod rig;

use teamwire_core::{
    Actor, Clock as _, DeliveryState, EventType, MessageType, Recipient, ReportState, Role, Route,
    TurnEnd, TurnEndReply, Ulid,
};
use teamwire_report::{on_tool_use, on_turn_end};
use teamwire_store::{DeliveryUpdate, NewMessage};

use kit::{stop, team};
use rig::Rig;

const MARKER: &str = "violet-73";

/// A body of `len` printable characters whose only marker is at its end.
fn body(len: usize) -> String {
    format!("{}{MARKER}", "a".repeat(len - MARKER.len()))
}

/// A message from `sender` to `who`, stored, its delivery set to `state`, and
/// for a reprompt the member's next generation started, as `teamwire send`
/// does in one transaction. Returns the message's ulid and display id.
fn send(
    rig: &mut Rig,
    sender: &Actor,
    kind: MessageType,
    who: &Role,
    body: &str,
    state: DeliveryState,
) -> (Ulid, String) {
    let at = rig.clock.now();
    let recipient = Recipient::Member(who.clone());
    rig.store
        .write(|tx| {
            let stored = tx.insert_message(
                at,
                &NewMessage {
                    sender: sender.clone(),
                    kind,
                    re: None,
                    commit: None,
                    body: body.to_owned(),
                    client_key: None,
                    recipients: vec![recipient.clone()],
                },
            )?;
            let id = stored.message.display_id.to_string();
            if kind == MessageType::Reprompt {
                tx.bump_generation(at, who, &id)?;
            }
            tx.set_delivery_state(
                at,
                &stored.message.ulid,
                &recipient,
                &DeliveryUpdate {
                    state,
                    route: Some(Route::HostTyping),
                    error: None,
                    attempts: 1,
                },
            )?;
            Ok((stored.message.ulid, id))
        })
        .unwrap()
}

fn reprompt(rig: &mut Rig, who: &Role, len: usize) -> (Ulid, String) {
    send(
        rig,
        &Actor::Lead,
        MessageType::Reprompt,
        who,
        &body(len),
        DeliveryState::HandedOff,
    )
}

fn ack(rig: &mut Rig, who: &Role, message: &Ulid) {
    let at = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.ack_delivery(at, message, &Recipient::Member(who.clone()))
                .map(|_| ())
        })
        .unwrap();
}

fn generation(rig: &Rig, who: &Role) -> u32 {
    rig.store
        .read(|tx| tx.member(who))
        .unwrap()
        .unwrap()
        .generation
}

/// A stop that a teamwire hold started: the harness says so.
fn held_stop(session: &str) -> TurnEnd {
    TurnEnd {
        hold_already_active: true,
        ..stop(session)
    }
}

/// The corrective holds recorded for `who`.
fn corrective_holds(rig: &Rig) -> Vec<serde_json::Value> {
    rig.events(EventType::MemberHeld)
        .into_iter()
        .filter_map(|e| e.data.get("corrective").cloned())
        .collect()
}

fn report_ok(rig: &Rig, who: &Role) {
    rig.write_report(
        who,
        &format!("Summary: did it, {MARKER}.\n\nDecisions I made: none\n"),
    );
}

fn hold_note(reply: TurnEndReply) -> String {
    match reply {
        TurnEndReply::Hold(note) => note,
        TurnEndReply::Continue => unreachable!("expected a hold, got none"),
    }
}

#[test]
fn a_clipped_reprompt_reminded_after_a_tool_still_gets_one_hold_at_the_stop() {
    let (mut rig, api) = team();
    let (_, id) = reprompt(&mut rig, &api, 301);
    // The tool hook's reminder marks the row reminded, and the member goes on
    // without reading it.
    let reminder = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(reminder.contains(&id), "{reminder}");

    let note = hold_note(on_turn_end(&mut rig.ctx(), &api, &stop("api-1")));
    assert!(
        note.contains(&format!("Reprompt {id} from the lead")),
        "{note}"
    );
    assert!(note.contains("still unread"), "{note}");
    assert!(note.contains("mcp__teamwire__team_inbox"), "{note}");
    assert!(note.contains("done, blocked or failed"), "{note}");
    assert!(!note.contains(MARKER), "{note}");
    assert_eq!(rig.completions(&api), 0, "a hold records no result");

    // The turn the hold started ends without reading it: no second hold,
    // and no hold on a later plain stop of the same generation either.
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &held_stop("api-1")),
        TurnEndReply::Continue
    ));
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    let holds = corrective_holds(&rig);
    assert_eq!(holds.len(), 1, "{holds:?}");
    assert_eq!(holds[0]["message"], id.as_str());
    assert_eq!(holds[0]["generation"], 2);
    assert_eq!(holds[0]["reason"], "unread");
    assert_eq!(rig.completions(&api), 0);
}

#[test]
fn a_read_reprompt_with_no_result_in_its_generation_gets_one_hold() {
    let (mut rig, api) = team();
    // A result of generation 1 is not a result of the reprompt's generation.
    report_ok(&rig, &api);
    rig.complete_now(&api);
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    let (message, id) = reprompt(&mut rig, &api, 301);
    ack(&mut rig, &api, &message);

    let note = hold_note(on_turn_end(&mut rig.ctx(), &api, &stop("api-1")));
    assert!(
        note.contains(&format!(
            "Reprompt {id} from the lead started a new generation"
        )),
        "{note}"
    );
    assert!(note.contains("no result"), "{note}");
    assert!(note.contains("done, blocked or failed"), "{note}");
    assert!(
        note.contains("an answer in chat or a finding is not a result"),
        "{note}"
    );
    assert_eq!(rig.completions(&api), 1, "only the generation-1 result");
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    let holds = corrective_holds(&rig);
    assert_eq!(holds.len(), 1, "{holds:?}");
    assert_eq!(holds[0]["reason"], "no_result");
    assert_eq!(holds[0]["generation"], 2);
}

#[test]
fn reading_the_reprompt_and_recording_done_delivers_exactly_one_result_of_its_generation() {
    let (mut rig, api) = team();
    let (message, _) = reprompt(&mut rig, &api, 301);
    on_tool_use(&mut rig.ctx(), &api);
    hold_note(on_turn_end(&mut rig.ctx(), &api, &stop("api-1")));

    // The held turn reads the inbox, updates the report and finishes.
    ack(&mut rig, &api, &message);
    report_ok(&rig, &api);
    rig.complete_now(&api);
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &held_stop("api-1")),
        TurnEndReply::Continue
    ));
    let newest = rig.newest(&api);
    assert_eq!(newest.completion.generation, 2);
    assert_eq!(generation(&rig, &api), 2);
    assert_eq!(newest.completion.report_state, ReportState::Delivered);
    assert_eq!(rig.completions(&api), 1);
    let typed = rig.typed_to_lead();
    assert_eq!(
        typed
            .iter()
            .filter(|l| l.contains("api finished (done)"))
            .count(),
        1,
        "{typed:?}"
    );
    let report = std::fs::read_to_string(rig.team_dir().join("reports/api.md")).unwrap();
    assert!(report.contains(MARKER));
    // A later stop delivers nothing again and holds nothing.
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    assert_eq!(rig.typed_to_lead().len(), typed.len());
}

#[test]
fn a_member_that_read_the_reprompt_and_finished_is_not_held() {
    let (mut rig, api) = team();
    let (message, _) = reprompt(&mut rig, &api, 301);
    ack(&mut rig, &api, &message);
    report_ok(&rig, &api);
    rig.complete_now(&api);
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    assert!(corrective_holds(&rig).is_empty());
}

#[test]
fn a_short_reprompt_typed_whole_is_read_and_held_only_for_a_missing_result() {
    let (mut rig, api) = team();
    let (_, id) = reprompt(&mut rig, &api, 300);
    let note = hold_note(on_turn_end(&mut rig.ctx(), &api, &stop("api-1")));
    // The line carried all of it, so it is not unread (ADR 0021).
    assert!(!note.contains("still unread"), "{note}");
    assert!(
        note.contains(&format!("Reprompt {id} from the lead started")),
        "{note}"
    );

    let (mut rig, api) = team();
    reprompt(&mut rig, &api, 300);
    report_ok(&rig, &api);
    rig.complete_now(&api);
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
}

#[test]
fn a_member_waiting_for_an_answer_is_not_held_for_its_result() {
    let (mut rig, api) = team();
    let (message, _) = reprompt(&mut rig, &api, 301);
    ack(&mut rig, &api, &message);
    // The member asked the lead and waits for the answer.
    let at = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.insert_message(
                at,
                &NewMessage {
                    sender: Actor::Member(api.clone()),
                    kind: MessageType::Question,
                    re: None,
                    commit: None,
                    body: "which port?".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Lead],
                },
            )
            .map(|_| ())
        })
        .unwrap();
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    assert!(corrective_holds(&rig).is_empty());
}

#[test]
fn a_generation_a_note_or_a_peer_started_gets_no_corrective_hold() {
    // A long note from the lead wakes a finished member (ADR 0043): that is
    // not a reprompt, so only the ordinary once-per-message hold applies.
    let (mut rig, api) = team();
    report_ok(&rig, &api);
    rig.complete_now(&api);
    on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));
    send(
        &mut rig,
        &Actor::Lead,
        MessageType::Note,
        &api,
        &body(301),
        DeliveryState::HandedOff,
    );
    on_tool_use(&mut rig.ctx(), &api);
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));

    // A peer's finding in generation 1 is information, not a reprompt.
    let (mut rig, api) = team();
    rig.add_member("web", "fake", "web-1");
    send(
        &mut rig,
        &Actor::Member(Role::parse("web").unwrap()),
        MessageType::Finding,
        &api,
        &body(301),
        DeliveryState::HandedOff,
    );
    on_tool_use(&mut rig.ctx(), &api);
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    assert!(corrective_holds(&rig).is_empty());
}

#[test]
fn a_closing_team_holds_nobody() {
    let (mut rig, api) = team();
    reprompt(&mut rig, &api, 301);
    on_tool_use(&mut rig.ctx(), &api);
    let at = rig.clock.now();
    let me = teamwire_core::platform::ProcessRef {
        pid: teamwire_core::Pid::new(std::process::id()),
        started: None,
    };
    rig.store
        .write(|tx| tx.begin_close(at, me).map(|_| ()))
        .unwrap();
    assert!(matches!(
        on_turn_end(&mut rig.ctx(), &api, &stop("api-1")),
        TurnEndReply::Continue
    ));
    assert!(corrective_holds(&rig).is_empty());
}
