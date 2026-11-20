//! The report check: who is told about report gaps, and how often.
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
    CompletionStatus, EventType, PlatformError, PlatformErrorCode, ReportState, RunOutput,
    TurnEndReply,
};
use teamwire_report::{DoneReport, done, on_turn_end};

use kit::{data_str, stop, team};

#[test]
fn no_hook_harness_told_once() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();

    // No report file at all: the first call stores nothing and says why.
    let first = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    let DoneReport::NotStored {
        stored,
        gaps,
        report,
    } = first
    else {
        unreachable!("the first call must not store")
    };
    assert!(!stored);
    assert!(gaps.report_missing);
    assert!(report.contains("reports/api.md"), "{report}");
    // No turn end delivers this result: the member calls done again.
    assert!(report.contains("call team_done again"), "{report}");
    assert!(!report.contains("stop again"), "{report}");
    assert_eq!(rig.completions(&api), 0);
    let refused = rig.events(EventType::MemberCompletionRefused);
    assert_eq!(refused.len(), 1);
    assert_eq!(data_str(&refused[0], "reason"), Some("report_gaps"));

    // The second call stores whatever the report says.
    let second = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    assert!(matches!(second, DoneReport::Stored { n: 1, .. }));
    assert_eq!(rig.completions(&api), 1);
}

#[test]
fn no_hook_harness_with_a_complete_report_is_stored_at_once() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();
    rig.write_report(&api, "notes\n\nDecisions I made: none\n");
    let report = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    assert!(matches!(report, DoneReport::Stored { .. }));
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn a_harness_with_a_turn_end_hook_is_not_checked_by_done() {
    let (mut rig, api) = team();
    // No report file, but the Stop hook will run the check at turn end.
    let report = done(&mut rig.ctx(), &api, CompletionStatus::Done, "finished").unwrap();
    assert!(matches!(report, DoneReport::Stored { .. }));
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn a_new_generation_is_told_again() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();
    let told = |rig: &mut rig::Rig| {
        matches!(
            done(&mut rig.ctx(), &api, CompletionStatus::Done, "x").unwrap(),
            DoneReport::NotStored { .. }
        )
    };
    assert!(told(&mut rig));
    assert!(!told(&mut rig));
    rig.store
        .write(|tx| tx.bump_generation(kit::at(9), &api, "again").map(|_| ()))
        .unwrap();
    assert!(told(&mut rig), "generation 2 is told once more");
}

fn reply(rig: &mut rig::Rig, who: &teamwire_core::Role) -> TurnEndReply {
    on_turn_end(&mut rig.ctx(), who, &stop("api-1"))
}

fn held_note(reply: TurnEndReply) -> String {
    match reply {
        TurnEndReply::Hold(note) => note,
        TurnEndReply::Continue => unreachable!("expected a hold"),
    }
}

#[test]
fn hold_once_then_deliver() {
    let (mut rig, api) = team();
    rig.complete_now(&api);

    // A missing report holds the member once, with the facts.
    let note = held_note(reply(&mut rig, &api));
    assert!(note.contains("reports/api.md"), "{note}");
    assert!(note.contains("does not exist"), "{note}");
    assert!(rig.typed_to_lead().is_empty(), "the lead hears nothing yet");
    assert_eq!(
        rig.newest(&api).completion.report_state,
        ReportState::Pending
    );
    let held = &rig.events(EventType::MemberHeld)[0];
    assert_eq!(data_str(held, "hook"), Some("fake-stop"));
    assert!(held.data.get("hook_ms").is_some());

    // The next stop delivers whatever the report says.
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);
    assert_eq!(rig.events(EventType::MemberHeld).len(), 1);
}

#[test]
fn a_complete_report_is_delivered_at_the_first_stop() {
    let (mut rig, api) = team();
    rig.write_report(&api, "Found it.\n\nDecisions I made: none\n");
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);
}

#[test]
fn missing_decisions_list_is_named() {
    let (mut rig, api) = team();
    rig.write_report(&api, "Found it.\n");
    rig.complete_now(&api);
    let note = held_note(reply(&mut rig, &api));
    assert!(note.contains("Decisions I made"), "{note}");
    assert!(!note.contains("does not exist"), "{note}");
}

/// The output of `git log` for one commit authored after the member started.
fn one_commit(sha: &str) -> RunOutput {
    RunOutput {
        exit_code: 0,
        stdout: format!("{sha} 1790000100\n").into_bytes(),
        stderr: Vec::new(),
        timed_out: false,
    }
}

#[test]
fn commit_without_progress_line_is_held() {
    let mut rig = rig::Rig::new();
    rig.healthy("lead-1", 100);
    let api = rig.add_worktree_member("api", "api-1", "0123456789abcdef");
    rig.write_report(&api, "- 9999999 something else\n\nDecisions I made: none\n");
    rig.complete_now(&api);
    rig.platform
        .queue_run(Ok(one_commit("abcdef0123456789abcdef0123456789abcdef01")));
    let note = held_note(reply(&mut rig, &api));
    assert!(note.contains("abcdef0"), "{note}");
    assert!(rig.typed_to_lead().is_empty());
}

#[test]
fn a_progress_line_for_each_commit_is_enough() {
    let mut rig = rig::Rig::new();
    rig.healthy("lead-1", 100);
    let api = rig.add_worktree_member("api", "api-1", "0123456789abcdef");
    rig.write_report(
        &api,
        "- abcdef0 did it | left: nothing\n\nDecisions I made: none\n",
    );
    rig.complete_now(&api);
    rig.platform
        .queue_run(Ok(one_commit("abcdef0123456789abcdef0123456789abcdef01")));
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);
}

#[test]
fn check_error_means_no_gaps() {
    let mut rig = rig::Rig::new();
    rig.healthy("lead-1", 100);
    let api = rig.add_worktree_member("api", "api-1", "0123456789abcdef");
    // The report lacks its decisions list, but the check itself fails.
    rig.write_report(&api, "notes only\n");
    rig.complete_now(&api);
    rig.platform.queue_run(Err(PlatformError::new(
        PlatformErrorCode::Io,
        "git is not installed",
    )));
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert!(
        !rig.platform.runs().is_empty(),
        "the check did run and fail"
    );
    assert_eq!(rig.typed_to_lead().len(), 1, "delivered, not held");
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn unread_messages_hold_once_and_do_not_use_up_the_report_hold() {
    let (mut rig, api) = team();
    let id = rig.unread_from_lead(&api, "please also check the docs");

    // Stopping with no completion yet: held for the unread message only.
    let note = held_note(reply(&mut rig, &api));
    assert!(
        note.contains(&id) && note.contains("teamwire inbox"),
        "{note}"
    );
    assert!(
        note.contains(
            "Read them with the team_inbox tool, or with the `teamwire inbox` command your contract names. \
             If one changes your result, update your report and record done again; otherwise stop"
        ),
        "{note}"
    );
    assert_eq!(
        reply(&mut rig, &api),
        TurnEndReply::Continue,
        "each id once"
    );
    assert_eq!(
        rig.store
            .read(|tx| tx.member(&api))
            .unwrap()
            .unwrap()
            .gate_told_generation,
        None
    );

    // The report-gap hold is still available afterwards.
    rig.complete_now(&api);
    let note = held_note(reply(&mut rig, &api));
    assert!(note.contains("does not exist"), "{note}");
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);
}

#[test]
fn a_member_woken_after_its_result_is_told_to_record_done_again() {
    let (mut rig, api) = team();
    rig.write_report(&api, "Found it.\n\nDecisions I made: none\n");
    rig.complete_now(&api);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(
        rig.typed_to_lead().len(),
        1,
        "the first result was delivered"
    );

    // A message to a finished member starts a new generation (ADR 0043), and
    // the watch waits for a result in it, even when nothing changed.
    rig.store
        .write(|tx| tx.bump_generation(kit::at(9), &api, "woken").map(|_| ()))
        .unwrap();
    rig.unread_from_lead(&api, "fyi, no action needed");
    let note = held_note(reply(&mut rig, &api));
    assert!(
        note.contains(
            "otherwise stop, and a result you recorded in this turn goes to the lead. \
             If you recorded no result in this turn, record done again before you stop, \
             even when nothing changed."
        ),
        "{note}"
    );
}

#[test]
fn gaps_and_unread_messages_share_one_note() {
    let (mut rig, api) = team();
    let id = rig.unread_from_lead(&api, "look at this");
    rig.complete_now(&api);
    let note = held_note(reply(&mut rig, &api));
    assert!(
        note.contains("does not exist") && note.contains(&id),
        "{note}"
    );
    assert_eq!(rig.events(EventType::MemberHeld).len(), 1);
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
}

#[test]
fn no_hold_at_all_when_the_stop_was_already_held() {
    let (mut rig, api) = team();
    rig.unread_from_lead(&api, "look at this");
    rig.complete_now(&api);
    let mut t = stop("api-1");
    t.hold_already_active = true;
    assert_eq!(
        on_turn_end(&mut rig.ctx(), &api, &t),
        TurnEndReply::Continue
    );
    assert!(rig.events(EventType::MemberHeld).is_empty());
}

#[test]
fn a_hookless_harness_gets_its_result_file_from_done() {
    let (mut rig, api) = team();
    rig.no_turn_end_hook();
    rig.write_report(&api, "Decisions I made: none\n");
    rig.complete_now(&api);
    let text = std::fs::read_to_string(rig.team_dir().join("outbox/api.md")).unwrap();
    assert!(text.contains("teamwire:generated"), "{text}");
    assert!(text.contains("not available from this harness"), "{text}");
}

#[test]
fn a_hooked_member_gets_its_result_file_from_the_stop_hook_not_from_done() {
    let (mut rig, api) = team();
    rig.complete_now(&api);
    assert!(!rig.team_dir().join("outbox/api.md").exists());
}

#[test]
fn a_failing_report_check_is_recorded() {
    let mut rig = rig::Rig::new();
    rig.healthy("lead-1", 100);
    let api = rig.add_worktree_member("api", "api-1", "0123456789abcdef");
    rig.write_report(&api, "notes only\n");
    rig.complete_now(&api);
    rig.platform.queue_run(Err(PlatformError::new(
        PlatformErrorCode::Io,
        "git is not installed",
    )));
    // Delivered anyway (INV-17), with the cause on the result event.
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1);
    let delivered = rig.events(EventType::ResultDelivered);
    let error = data_str(&delivered[0], "report_check_error").unwrap();
    assert!(error.contains("git is not installed"), "{error}");
}

#[test]
fn a_corrupt_event_beside_a_pending_delivery_does_not_stop_the_result() {
    let (mut rig, api) = team();
    rig.write_report(&api, "Found it.\n\nDecisions I made: none\n");
    rig.complete_now(&api);
    let at = teamwire_core::Clock::now(&*rig.clock);
    let recipient = teamwire_core::Recipient::Member(api.clone());
    rig.store
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
            tx.connection().execute(
                "INSERT INTO events (ulid, time, type, subject, data, writer_version)
                 VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'not a time', 'delivery.resent', ?1, '{}', 'x')",
                [stored.message.ulid.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1, "the result was delivered");
}

#[test]
fn a_message_body_that_fails_its_hash_does_not_stop_the_result() {
    let (mut rig, api) = team();
    rig.write_report(&api, "Found it.\n\nDecisions I made: none\n");
    rig.complete_now(&api);
    let at = teamwire_core::Clock::now(&*rig.clock);
    let recipient = teamwire_core::Recipient::Member(api.clone());
    rig.store
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
            tx.connection().execute(
                "UPDATE messages SET body = 'changed after the hash' WHERE ulid = ?1",
                [stored.message.ulid.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(reply(&mut rig, &api), TurnEndReply::Continue);
    assert_eq!(rig.typed_to_lead().len(), 1, "the result was delivered");
}

/// The hook's name in the metrics follows the member's own harness.
#[test]
fn the_hold_event_names_the_hook_of_the_members_harness() {
    for (harness, hook) in [("pi", "pi-stop"), ("claude-code", "claude-stop")] {
        let (mut rig, _) = team();
        let teamwire_store::Party::Member(web) = rig.add_member("web", harness, "web-1") else {
            unreachable!("add_member returns a member")
        };
        rig.complete_now(&web);
        let reply = on_turn_end(&mut rig.ctx(), &web, &stop("web-1"));
        assert!(matches!(reply, TurnEndReply::Hold(_)), "{harness}");
        let held = &rig.events(EventType::MemberHeld)[0];
        assert_eq!(data_str(held, "hook"), Some(hook));
    }
}
