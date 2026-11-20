//! The in-turn reminder of unread messages.
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

#[test]
fn reminds_once_per_message_id() {
    let (mut rig, api) = team();
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None, "nothing unread");

    let first = rig.unread_from_lead(&api, "one");
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(
        note.contains(&first) && note.contains("teamwire inbox"),
        "{note}"
    );
    assert!(
        note.contains("Read them with the team_inbox tool"),
        "{note}"
    );
    // A member in the middle of its turn is not told to stop.
    assert!(!note.contains("otherwise stop"), "{note}");
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None, "each id once");

    let second = rig.unread_from_lead(&api, "two");
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&second) && !note.contains(&first), "{note}");
    assert_eq!(rig.events(EventType::DeliveryReminded).len(), 2);
}

#[test]
fn a_read_message_is_not_reminded() {
    let (mut rig, api) = team();
    rig.unread_from_lead(&api, "one");
    let members = rig
        .store
        .read(|tx| {
            tx.inbox(
                &teamwire_core::Recipient::Member(api.clone()),
                &[teamwire_core::DeliveryState::HandedOff],
            )
        })
        .unwrap();
    let ulid = members.items[0].message.ulid;
    let at = kit::at(5);
    rig.store
        .write(|tx| {
            tx.ack_delivery(at, &ulid, &teamwire_core::Recipient::Member(api.clone()))
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None);
}

#[test]
fn a_pending_delivery_older_than_a_minute_is_reminded_once() {
    let (mut rig, api) = team();
    // A sender that crashed after the commit left the row pending.
    let at = teamwire_core::Clock::now(&*rig.clock);
    let recipient = teamwire_core::Recipient::Member(api.clone());
    let display = rig
        .store
        .write(|tx| {
            let stored = tx.insert_message(
                at,
                &teamwire_store::NewMessage {
                    sender: teamwire_core::Actor::Lead,
                    kind: teamwire_core::MessageType::Note,
                    re: None,
                    commit: None,
                    body: "left pending".to_owned(),
                    client_key: None,
                    recipients: vec![recipient],
                },
            )?;
            Ok(stored.message.display_id.to_string())
        })
        .unwrap();
    rig.clock.advance(std::time::Duration::from_secs(30));
    assert_eq!(
        on_tool_use(&mut rig.ctx(), &api),
        None,
        "its sender may still be delivering"
    );
    rig.clock.advance(std::time::Duration::from_secs(31));
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&display), "{note}");
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None, "each id once");
}

#[test]
fn a_message_the_typed_line_carried_whole_is_not_reminded() {
    let (mut rig, api) = team();
    rig.message_from_lead(
        &api,
        "rebase on main",
        teamwire_core::DeliveryState::HandedOff,
    );
    assert_eq!(
        on_tool_use(&mut rig.ctx(), &api),
        None,
        "the line held it all"
    );
    assert!(rig.events(EventType::DeliveryReminded).is_empty());
}

#[test]
fn a_short_message_the_host_never_took_is_still_reminded() {
    let (mut rig, api) = team();
    let id = rig.message_from_lead(
        &api,
        "rebase on main",
        teamwire_core::DeliveryState::Undelivered,
    );
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&id), "{note}");
}

/// A message from the lead to `who`, held for a handoff (ADR 0043 decision
/// 11): `undelivered` with `held_for_handoff`, never typed.
fn held_from_lead(rig: &mut rig::Rig, who: &teamwire_core::Role) -> String {
    let held = rig.message_from_lead(
        who,
        "one more thing",
        teamwire_core::DeliveryState::Undelivered,
    );
    let recipient = teamwire_core::Recipient::Member(who.clone());
    let at = kit::at(3);
    rig.store
        .write(|tx| {
            let message = tx
                .reader()
                .message_by_display_id(&teamwire_core::DisplayId::parse(&held).unwrap())?
                .unwrap();
            tx.set_delivery_state(
                at,
                &message.ulid,
                &recipient,
                &teamwire_store::DeliveryUpdate {
                    state: teamwire_core::DeliveryState::Undelivered,
                    route: None,
                    error: Some(teamwire_core::DeliveryError::new(
                        teamwire_core::DeliveryErrorCode::HeldForHandoff,
                        "the member's newest result asked for a handoff",
                    )),
                    attempts: 0,
                },
            )
        })
        .unwrap();
    held
}

/// A held message waits for the fresh session: the old session's in-turn
/// reminder never names it, so that session does no more work for it. The
/// message beside it, left `undelivered` with no hold, is still named.
#[test]
fn a_message_held_for_a_handoff_is_not_reminded() {
    let (mut rig, api) = team();
    let held = held_from_lead(&mut rig, &api);
    assert_eq!(on_tool_use(&mut rig.ctx(), &api), None);
    let other = rig.message_from_lead(
        &api,
        "rebase on main",
        teamwire_core::DeliveryState::Undelivered,
    );
    let note = on_tool_use(&mut rig.ctx(), &api).unwrap();
    assert!(note.contains(&other) && !note.contains(&held), "{note}");
}

/// The old session's Stop hook does not hold its stop for a held message.
#[test]
fn a_message_held_for_a_handoff_does_not_hold_the_stop() {
    let (mut rig, api) = team();
    held_from_lead(&mut rig, &api);
    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &kit::stop("api-1")),
        TurnEndReply::Continue
    );
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

// ADR 0059: the one corrective hold of a generation that a reprompt from
// the lead started. A reminder after a tool call is not reading the
// reprompt, and a chat answer is not a result.

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

/// Records `status` with `summary` for `who` now, through the member's own
/// `done`.
fn done_with(rig: &mut Rig, who: &Role, status: teamwire_core::CompletionStatus, summary: &str) {
    report_ok(rig, who);
    teamwire_report::done(&mut rig.ctx(), who, status, summary).unwrap();
}

#[test]
fn a_later_reprompt_whose_generation_was_undone_leaves_the_first_reprompts_hold() {
    let (mut rig, api) = team();
    let (_, id) = reprompt(&mut rig, &api, 301);
    on_tool_use(&mut rig.ctx(), &api);
    // A second reprompt bumps to generation 3, its delivery fails, and
    // generation 2 is given back: the first reprompt still owns it.
    let at = rig.clock.now();
    let started = rig
        .store
        .read(|tx| tx.member(&api))
        .unwrap()
        .unwrap()
        .gen_started_at;
    rig.store
        .write(|tx| {
            tx.bump_generation(at, &api, "m002")?;
            tx.restore_generation(at, &api, 2, started, EventType::MemberReprompted)
        })
        .unwrap();
    assert_eq!(generation(&rig, &api), 2);
    let note = hold_note(on_turn_end(&mut rig.ctx(), &api, &stop("api-1")));
    assert!(
        note.contains(&format!("Reprompt {id} from the lead")),
        "{note}"
    );
    let holds = corrective_holds(&rig);
    assert_eq!(holds.len(), 1, "{holds:?}");
    assert_eq!(holds[0]["message"], id.as_str());
}

#[test]
fn a_damaged_reprompt_does_not_stop_the_recorded_result_from_being_delivered() {
    let (mut rig, api) = team();
    let (_, id) = reprompt(&mut rig, &api, 301);
    done_with(
        &mut rig,
        &api,
        teamwire_core::CompletionStatus::Done,
        "finished the api",
    );
    rig.store
        .write(|tx| {
            tx.connection().execute(
                "UPDATE messages SET body = 'damaged' WHERE display_id = ?1",
                [id.as_str()],
            )?;
            Ok(())
        })
        .unwrap();
    let reply = on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));
    assert!(matches!(reply, TurnEndReply::Continue), "{reply:?}");
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Delivered
    );
    assert_eq!(rig.typed_to_lead().len(), 1, "{:?}", rig.typed_to_lead());
}

#[test]
fn a_handoff_or_a_question_round_is_not_held_for_the_reprompt() {
    for (status, summary) in [
        (
            teamwire_core::CompletionStatus::Done,
            "handoff: fresh session",
        ),
        (
            teamwire_core::CompletionStatus::Blocked,
            "1 question for the parent. Which port?",
        ),
        (
            teamwire_core::CompletionStatus::Blocked,
            "handoff: 2 questions for the parent. Which port, which host?",
        ),
    ] {
        let (mut rig, api) = team();
        reprompt(&mut rig, &api, 301);
        on_tool_use(&mut rig.ctx(), &api);
        done_with(&mut rig, &api, status, summary);
        let reply = on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));
        assert!(
            corrective_holds(&rig).is_empty(),
            "{summary}: {:?}",
            corrective_holds(&rig)
        );
        if let TurnEndReply::Hold(note) = reply {
            assert!(!note.contains("Reprompt m001"), "{summary}: {note}");
        }
    }
}

#[test]
fn a_member_that_may_not_send_is_not_told_to_ask_a_question() {
    let (mut rig, api) = team();
    rig.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET kind = 'closer' WHERE role = 'api'", [])?;
            Ok(())
        })
        .unwrap();
    let (message, _) = reprompt(&mut rig, &api, 301);
    ack(&mut rig, &api, &message);
    let note = hold_note(on_turn_end(&mut rig.ctx(), &api, &stop("api-1")));
    assert!(note.contains("no result"), "{note}");
    assert!(!note.contains("question"), "{note}");
    assert!(note.contains("record blocked"), "{note}");
}

/// Stops of one generation that run at the same time, each on its own
/// connection, as hook processes do, record one corrective hold between
/// them, and only that stop is held for it.
#[test]
fn stops_at_the_same_time_record_one_corrective_hold() {
    let (mut rig, api) = team();
    reprompt(&mut rig, &api, 301);
    on_tool_use(&mut rig.ctx(), &api);
    let team = teamwire_core::TeamName::parse("alpha").unwrap();
    let start = std::sync::Barrier::new(8);
    let held: usize = std::thread::scope(|s| {
        let runs: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(|| {
                    let mut store = teamwire_store::Store::open(rig.home.path(), &team).unwrap();
                    let mut ctx = teamwire_store::Ctx {
                        home: rig.home.path(),
                        store: &mut store,
                        registry: &rig.registry,
                        platform: &rig.platform,
                        clock: &*rig.clock,
                        config: &rig.config,
                    };
                    start.wait();
                    match on_turn_end(&mut ctx, &api, &stop("api-1")) {
                        TurnEndReply::Hold(note) => usize::from(note.contains("Reprompt m001")),
                        TurnEndReply::Continue => 0,
                    }
                })
            })
            .collect();
        runs.into_iter().map(|r| r.join().unwrap()).sum()
    });
    assert_eq!(held, 1, "stops held for the reprompt");
    assert_eq!(
        corrective_holds(&rig).len(),
        1,
        "{:?}",
        corrective_holds(&rig)
    );
}

/// A pending handoff or question-round result is delivered at the stop with
/// the clipped reprompt unread, whether or not a tool hook reminded it
/// first. The reprompt stays unread for the next session or round. A plain
/// result in the same place is still held, as before.
#[test]
fn a_pending_handoff_or_question_round_is_delivered_with_the_reprompt_left_unread() {
    use teamwire_core::CompletionStatus::{Blocked, Done};
    let cases = [
        (Done, "handoff: fresh session", true),
        (Blocked, "1 question for the parent. Which port?", true),
        (Done, "finished the api", false),
    ];
    for (status, summary, waits) in cases {
        for reminded in [false, true] {
            let (mut rig, api) = team();
            let (ulid, _) = reprompt(&mut rig, &api, 301);
            if reminded {
                on_tool_use(&mut rig.ctx(), &api);
            }
            done_with(&mut rig, &api, status, summary);
            let reply = on_turn_end(&mut rig.ctx(), &api, &stop("api-1"));
            let case = format!("{summary}, reminded {reminded}: {reply:?}");
            let delivered = rig.newest(&api).completion.report_state == ReportState::Delivered;
            let row = rig
                .store
                .read(|tx| tx.delivery(&ulid, &Recipient::Member(api.clone())))
                .unwrap()
                .unwrap();
            assert_ne!(row.delivery.state, DeliveryState::Acked, "{case}");
            if waits {
                assert!(matches!(reply, TurnEndReply::Continue), "{case}");
                assert!(delivered, "{case}");
                assert_eq!(rig.typed_to_lead().len(), 1, "{case}");
                assert_eq!(row.reminded_at.is_some(), reminded, "{case}");
            } else {
                assert!(hold_note(reply).contains("m001"), "{summary}");
                assert!(!delivered, "{case}");
            }
        }
    }
}
