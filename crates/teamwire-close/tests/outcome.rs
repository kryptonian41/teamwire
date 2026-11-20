//! ADR 0059: the outcome checks of a whole-team close. A refused reprompt
//! starts no generation, so a close after it would archive a blocked or
//! older result as if it were the one the lead asked for.

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, TEAM};
use serde_json::{Value, json};
use teamwire_close::{CloseOpts, CloseTarget, Expected, close, close_observed, plan_close_with};
use teamwire_core::{
    Actor, Clock as _, CompletionStatus, EventType, FinalSource, Host as _, MemberKind,
    MessageType, Recipient, Role, TeamName, TeamState,
};
use teamwire_store::{NewCompletion, NewMessage, Store, StoreError, Tx};

fn role(name: &str) -> Role {
    Role::parse(name).unwrap_or_else(|e| unreachable!("{name} is a role: {e}"))
}

/// `short` in generation `generation` with a result of `status` there. Each
/// generation it moves up is started by a stored lead reprompt, as a send
/// starts it.
fn result_in(
    tx: &Tx<'_>,
    rig_now: teamwire_core::Timestamp,
    generation: u32,
    status: CompletionStatus,
) -> Result<(), StoreError> {
    let short = role("short");
    while tx
        .reader()
        .member(&short)?
        .is_some_and(|m| m.generation < generation)
    {
        let id = lead_reprompt(tx, rig_now, &short)?;
        tx.bump_generation(rig_now, &short, &id)?;
    }
    tx.insert_completion(
        rig_now,
        &NewCompletion {
            role: short,
            generation,
            status,
            summary: "stopped".to_owned(),
            final_message: None,
            final_source: FinalSource::None,
        },
    )?;
    Ok(())
}

/// Stores a lead reprompt to `to` and returns its display id.
fn lead_reprompt(
    tx: &Tx<'_>,
    now: teamwire_core::Timestamp,
    to: &Role,
) -> Result<String, StoreError> {
    let stored = tx.insert_message(
        now,
        &NewMessage {
            sender: Actor::Lead,
            kind: MessageType::Reprompt,
            re: None,
            commit: None,
            body: "the next part".to_owned(),
            client_key: None,
            recipients: vec![Recipient::Member(to.clone())],
        },
    )?;
    Ok(stored.message.display_id.to_string())
}

/// The team of the case that was seen: `short` blocked in generation 3,
/// then a reprompt the lead sent to a word that is no role, refused.
fn blocked_then_refused() -> Rig {
    let mut rig = Rig::new();
    rig.add_pane_member("short", MemberKind::Investigator);
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            result_in(tx, now, 3, CompletionStatus::Blocked)?;
            tx.record_event(
                now,
                EventType::MessageRefused,
                None,
                None,
                &json!({
                    "code": "unknown_recipient", "reason": "unknown_recipient",
                    "type": "reprompt", "sender": "lead", "to": ["members"],
                }),
            )
        })
        .unwrap_or_else(|e| unreachable!("the fixture is written: {e}"));
    rig
}

fn expecting(text: &str) -> CloseOpts {
    CloseOpts {
        expect: vec![Expected::parse(text).unwrap_or_else(|e| unreachable!("{text}: {e}"))],
        ..CloseOpts::default()
    }
}

fn place_is_open(rig: &Rig, name: &str) -> bool {
    rig.host
        .state(&rig.place_of(name), std::time::Duration::ZERO)
        .is_ok()
}

/// Nothing was taken away: the team is open, the place and the state are
/// there, and no archive was made.
fn untouched(rig: &Rig) {
    assert_eq!(rig.team().state, TeamState::Open);
    assert!(place_is_open(rig, "short"));
    assert!(rig.team_dir().join("team.db").is_file());
    assert!(!rig.archive_root().exists());
}

#[test]
fn a_bare_close_after_a_refused_send_refuses_in_the_plan_and_the_close() {
    let mut rig = blocked_then_refused();
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &CloseOpts::default());
    let why = plan.refusals.join("; ");
    assert!(why.contains("the lead's newest send was refused"), "{why}");
    assert!(
        why.contains("unknown_recipient; type reprompt; to members"),
        "{why}"
    );
    assert!(
        why.contains("--expect <role>:<generation>:<status>"),
        "{why}"
    );
    assert!(why.contains("--abandon"), "{why}");

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(report.steps.is_empty(), "{report:?}");
    assert!(
        report
            .refusals
            .iter()
            .any(|r| r.contains("newest send was refused")),
        "{report:?}"
    );
    untouched(&rig);
    assert_eq!(rig.events(EventType::CloseRefused).len(), 1);
    assert!(rig.events(EventType::TeamClosing).is_empty());
}

#[test]
fn a_message_the_lead_stored_after_the_refusal_answers_the_refused_send() {
    let mut rig = blocked_then_refused();
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.insert_message(
                now,
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: "thanks, closing".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Member(role("short"))],
                },
            )
            .map(|_| ())
        })
        .unwrap();
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &CloseOpts::default());
    let why = plan.refusals.join("; ");
    assert!(!why.contains("newest send was refused"), "{why}");
    // The blocked result still needs a decision.
    assert!(
        why.contains("short's current result is blocked (generation 3)"),
        "{why}"
    );
    // The note answers the newest-send check only. The reprompt it followed
    // is still unanswered for short: a note asks for no new work.
    assert!(
        why.contains("short's newest reprompt from the lead was refused (unknown_recipient"),
        "{why}"
    );
}

/// A team whose members are as given, with no send of the lead recorded at
/// all, refused or stored: a client that checked the call itself and never
/// reached teamwire leaves no row.
fn team_of(results: &[(&str, u32, Option<CompletionStatus>)]) -> Rig {
    let mut rig = Rig::new();
    for (name, _, _) in results {
        rig.add_pane_member(name, MemberKind::Investigator);
    }
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            for (name, generation, status) in results {
                let who = role(name);
                while tx
                    .reader()
                    .member(&who)?
                    .is_some_and(|m| m.generation < *generation)
                {
                    tx.bump_generation(now, &who, "m900")?;
                }
                if let Some(status) = status {
                    tx.insert_completion(
                        now,
                        &NewCompletion {
                            role: who,
                            generation: *generation,
                            status: *status,
                            summary: "stopped".to_owned(),
                            final_message: None,
                            final_source: FinalSource::None,
                        },
                    )?;
                }
            }
            Ok(())
        })
        .unwrap_or_else(|e| unreachable!("the fixture is written: {e}"));
    rig
}

#[test]
fn a_bare_close_refuses_a_blocked_or_failed_current_result_with_no_send_history() {
    for status in [CompletionStatus::Blocked, CompletionStatus::Failed] {
        let mut rig = team_of(&[("short", 3, Some(status))]);
        let said = format!("short's current result is {status} (generation 3)");
        let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &CloseOpts::default());
        assert!(plan.refusals.iter().any(|r| r.contains(&said)), "{plan:?}");
        assert!(
            plan.refusals
                .iter()
                .any(|r| r.contains(&format!("--expect short:3:{status}"))),
            "{plan:?}"
        );
        let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
        assert!(report.steps.is_empty(), "{report:?}");
        assert!(
            report.refusals.iter().any(|r| r.contains(&said)),
            "{report:?}"
        );
        untouched(&rig);
    }
}

#[test]
fn a_current_generation_with_no_result_after_an_older_one_refuses() {
    let mut rig = team_of(&[("short", 3, Some(CompletionStatus::Done))]);
    let now = rig.clock.now();
    rig.store
        .write(|tx| tx.bump_generation(now, &role("short"), "m002").map(|_| ()))
        .unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.iter().any(|r| r.contains(
            "short is in generation 4 with no result yet (its newest is generation 3, done)"
        )),
        "{report:?}"
    );
    untouched(&rig);
    // Its old result does not stand in for the new generation.
    let report = close(&mut rig.ctx(), CloseTarget::Team, expecting("short:3:done"));
    assert!(
        report
            .refusals
            .iter()
            .any(|r| r.contains("short is in generation 4, not 3")),
        "{report:?}"
    );
    untouched(&rig);
}

#[test]
fn an_expectation_about_another_member_does_not_accept_a_blocked_one() {
    let mut rig = team_of(&[
        ("short", 3, Some(CompletionStatus::Blocked)),
        ("web", 1, Some(CompletionStatus::Done)),
        ("quiet", 1, None),
    ]);
    let opts = expecting("web:1:done");
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
    assert!(
        plan.refusals
            .iter()
            .any(|r| r.contains("short's current result is blocked")),
        "{plan:?}"
    );
    // Only short is named: web is done and quiet never reported.
    assert_eq!(plan.refusals.len(), 1, "{plan:?}");
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert!(report.steps.is_empty(), "{report:?}");
    untouched(&rig);

    let report = close(
        &mut rig.ctx(),
        CloseTarget::Team,
        expecting("short:3:blocked"),
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty(), "{report:?}");
}

#[test]
fn a_close_whose_members_finished_done_or_never_reported_runs_as_before() {
    let mut rig = team_of(&[("web", 2, Some(CompletionStatus::Done)), ("quiet", 1, None)]);
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty(), "{report:?}");
    assert!(
        report
            .steps
            .iter()
            .all(|s| s.step != "expect" && s.step != "abandon")
    );
}

#[test]
fn a_blocked_generation_3_does_not_pass_for_a_required_done_in_generation_4() {
    let mut rig = blocked_then_refused();
    for (text, said) in [
        ("short:4:done", "short is in generation 3, not 4"),
        (
            "short:3:done",
            "short's result in generation 3 is blocked, not done",
        ),
        ("scout:1:done", "the team has no member scout"),
    ] {
        let opts = expecting(text);
        // The dry run says the same as the close.
        let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
        assert!(
            plan.refusals.iter().any(|r| r.contains(said)),
            "{text}: {plan:?}"
        );
        let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
        assert!(report.steps.is_empty(), "{text}: {report:?}");
        assert!(
            report.refusals.iter().any(|r| r.contains(said)),
            "{text}: {report:?}"
        );
        untouched(&rig);
    }
}

#[test]
fn an_expectation_names_a_result_a_member_records() {
    for bad in [
        "short:4:skipped",
        "short:4:clean",
        "short:4:SKIPPED",
        "short:x:done",
        "short:0:done",
        "short:4",
        "short",
        "bad role:4:done",
    ] {
        assert!(Expected::parse(bad).is_err(), "{bad}");
    }
    let e = Expected::parse("short:4:blocked").unwrap();
    assert_eq!(e.to_string(), "short:4:blocked");
}

#[test]
fn the_matching_result_closes_the_team_and_the_check_is_recorded() {
    let mut rig = blocked_then_refused();
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            result_in(tx, now, 4, CompletionStatus::Done)?;
            Ok(())
        })
        .unwrap();
    let opts = expecting("short:4:done");
    assert!(
        plan_close_with(&rig.ctx(), CloseTarget::Team, &opts)
            .refusals
            .is_empty()
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    let step = report.steps.iter().find(|s| s.step == "expect").unwrap();
    assert!(step.ok);
    assert_eq!(step.detail, "results as expected: short:4:done");
    assert!(report.archive.is_some_and(|a| a.is_dir()));
    assert!(!rig.team_dir().exists());
}

#[test]
fn abandon_closes_a_team_on_purpose_and_records_why() {
    let mut rig = blocked_then_refused();
    let opts = CloseOpts {
        abandon: Some("short is blocked on a missing key; the user stopped the work".to_owned()),
        keep_state: true,
        ..CloseOpts::default()
    };
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
    assert!(plan.refusals.is_empty(), "{plan:?}");
    assert!(plan.steps.iter().any(|s| s.step == "abandon"));
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(rig.team().state, TeamState::Closed);
    let recorded: Vec<String> = rig
        .events(EventType::CloseStep)
        .into_iter()
        .filter(|e| e.data.get("step").and_then(Value::as_str) == Some("abandon"))
        .filter_map(|e| {
            e.data
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        recorded,
        [
            "closing as it is (--abandon): short is blocked on a missing key; the user stopped the work"
        ]
    );
}

#[test]
fn a_result_that_changes_before_the_lock_stops_the_close_with_nothing_removed() {
    let mut rig = blocked_then_refused();
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            result_in(tx, now, 4, CompletionStatus::Done)?;
            Ok(())
        })
        .unwrap();
    let home = rig.home.path().to_path_buf();
    // Between the plan and the lock, a reprompt starts generation 5.
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        expecting("short:4:done"),
        &|step| {
            if step == "lock" {
                let mut store =
                    Store::open_existing(&home, &TeamName::parse(TEAM).unwrap()).unwrap();
                store
                    .write(|tx| tx.bump_generation(now, &role("short"), "m009").map(|_| ()))
                    .unwrap();
            }
        },
    );
    assert_eq!(report.stopped_at.as_deref(), Some("expect"), "{report:?}");
    let failed = report.steps.last().unwrap();
    assert!(!failed.ok);
    assert!(
        failed
            .detail
            .starts_with("the world changed under the lock: "),
        "{failed:?}"
    );
    assert!(
        failed.detail.contains("short is in generation 5, not 4"),
        "{failed:?}"
    );
    // The lock was given back and nothing was taken away.
    untouched(&rig);
}

#[test]
fn a_refused_send_is_answered_only_by_an_expectation_for_each_member_it_was_meant_for() {
    let mut rig = team_of(&[
        ("short", 2, Some(CompletionStatus::Done)),
        ("web", 1, Some(CompletionStatus::Done)),
    ]);
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.record_event(
                now,
                EventType::MessageRefused,
                None,
                None,
                &json!({"code": "unknown_recipient", "type": "reprompt", "sender": "lead",
                        "to": ["members"]}),
            )
        })
        .unwrap();
    // `members` names no member, so the send was meant for any of them.
    let one = plan_close_with(&rig.ctx(), CloseTarget::Team, &expecting("web:1:done"));
    assert!(
        one.refusals
            .iter()
            .any(|r| r.contains("newest send was refused")),
        "{one:?}"
    );
    let both = CloseOpts {
        expect: vec![
            Expected::parse("web:1:done").unwrap(),
            Expected::parse("short:2:done").unwrap(),
        ],
        ..CloseOpts::default()
    };
    // The expectations answer the newest-send check, but each member's
    // reprompt is still unanswered: only a new reprompt to it answers that.
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &both);
    assert!(
        !plan
            .refusals
            .iter()
            .any(|r| r.contains("newest send was refused")),
        "{plan:?}"
    );
    for who in ["short", "web"] {
        assert!(
            plan.refusals.iter().any(|r| r.starts_with(&format!(
                "{who}'s newest reprompt from the lead was refused (unknown_recipient"
            ))),
            "{who}: {plan:?}"
        );
    }
    let report = close(&mut rig.ctx(), CloseTarget::Team, both);
    assert!(report.steps.is_empty(), "{report:?}");
    untouched(&rig);

    rig.store
        .write(|tx| {
            for (who, generation) in [("short", 3), ("web", 2)] {
                let id = lead_reprompt(tx, now, &role(who))?;
                tx.bump_generation(now, &role(who), &id)?;
                tx.insert_completion(
                    now,
                    &NewCompletion {
                        role: role(who),
                        generation,
                        status: CompletionStatus::Done,
                        summary: "finished".to_owned(),
                        final_message: None,
                        final_source: FinalSource::None,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let both = CloseOpts {
        expect: vec![
            Expected::parse("web:2:done").unwrap(),
            Expected::parse("short:3:done").unwrap(),
        ],
        ..CloseOpts::default()
    };
    let report = close(&mut rig.ctx(), CloseTarget::Team, both);
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty(), "{report:?}");
}

fn named(expect: &[&str]) -> CloseOpts {
    CloseOpts {
        team: teamwire_core::TeamName::parse(TEAM).ok(),
        expect: expect
            .iter()
            .map(|e| Expected::parse(e).unwrap_or_else(|why| unreachable!("{e}: {why}")))
            .collect(),
        ..CloseOpts::default()
    }
}

/// A store with no team row (a launch killed before it wrote one) is
/// deleted by a plain close. An expectation cannot hold there, so it
/// refuses, in the plan and the close, and the directory stays.
#[test]
fn an_expectation_refuses_on_a_store_with_nothing_recorded() {
    let mut rig = Rig::without_team_row();
    let dir = rig.team_dir();
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named(&["missing:4:done"]));
    let why = plan.refusals.join("; ");
    assert!(
        why.contains("--expect missing:4:done: the team has no member missing"),
        "{plan:?}"
    );
    let report = close(
        &mut rig.ctx(),
        CloseTarget::Team,
        named(&["missing:4:done"]),
    );
    assert!(!report.refusals.is_empty(), "{report:?}");
    assert!(report.steps.is_empty(), "{report:?}");
    assert!(dir.join("team.db").is_file());
}

/// A team closed with `--keep-state` still has its store. Closing it again
/// with an expectation checks the expectation: a wrong one refuses and
/// keeps the store, the matching one finishes the close.
#[test]
fn an_expectation_is_checked_when_a_kept_closed_team_is_closed_again() {
    let mut rig = team_of(&[("short", 1, Some(CompletionStatus::Done))]);
    let dir = rig.team_dir();
    let kept = CloseOpts {
        keep_state: true,
        ..CloseOpts::default()
    };
    let report = close(&mut rig.ctx(), CloseTarget::Team, kept);
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(rig.team().state, TeamState::Closed);

    for wrong in ["missing:4:done", "short:2:done"] {
        let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named(&[wrong]));
        assert!(
            plan.refusals
                .iter()
                .any(|r| r.starts_with(&format!("--expect {wrong}:"))),
            "{wrong}: {plan:?}"
        );
        let report = close(&mut rig.ctx(), CloseTarget::Team, named(&[wrong]));
        assert!(!report.refusals.is_empty(), "{wrong}: {report:?}");
        assert!(dir.join("team.db").is_file(), "{wrong}");
    }
    let report = close(&mut rig.ctx(), CloseTarget::Team, named(&["short:1:done"]));
    assert!(report.refusals.is_empty(), "{report:?}");
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!dir.exists());
}

/// The lead's newest reprompt was stored, its delivery failed, and its
/// generation was given back: the member still shows its older `done`. A
/// bare close refuses, because that result is not an answer to the
/// reprompt. An expectation of that older result does not answer it either;
/// `--abandon` closes the team as it is.
#[test]
fn a_bare_close_refuses_after_a_reprompt_whose_generation_was_undone() {
    let mut rig = team_of(&[("short", 2, Some(CompletionStatus::Done))]);
    let now = rig.clock.now();
    let short = role("short");
    rig.store
        .write(|tx| {
            let member = tx.reader().member(&short)?;
            let started_at = member.map_or(now, |m| m.gen_started_at);
            let stored = tx.insert_message(
                now,
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Reprompt,
                    re: None,
                    commit: None,
                    body: "part two".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Member(short.clone())],
                },
            )?;
            let id = stored.message.display_id.to_string();
            tx.bump_generation(now, &short, &id)?;
            tx.restore_generation(now, &short, 2, started_at, EventType::MemberReprompted)
        })
        .unwrap();
    for opts in [CloseOpts::default(), named(&[])] {
        let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
        let why = plan.refusals.join("; ");
        assert!(
            why.contains("short's newest reprompt from the lead, m001, started no generation"),
            "{why}"
        );
    }
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(!report.refusals.is_empty(), "{report:?}");
    untouched(&rig);
    // An expectation of the older result does not answer the reprompt.
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &expecting("short:2:done"));
    assert!(
        plan.refusals
            .iter()
            .any(|r| r.contains("short's newest reprompt from the lead, m001")),
        "{plan:?}"
    );
    let abandon = CloseOpts {
        abandon: Some("short's work stopped".to_owned()),
        ..CloseOpts::default()
    };
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &abandon);
    assert!(plan.refusals.is_empty(), "{plan:?}");
}
