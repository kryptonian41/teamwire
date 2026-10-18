//! A store that `teamwire launch` made and was killed before it wrote
//! anything: close deletes the directory, and only when the store holds no
//! team row and no member row.
#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, TEAM};
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close_with};
use teamwire_core::{Actor, Clock, EventType, MemberKind, MessageType, Recipient, TeamName};
use teamwire_store::NewMessage;

fn named() -> CloseOpts {
    CloseOpts {
        team: TeamName::parse(TEAM).ok(),
        ..CloseOpts::default()
    }
}

#[test]
fn a_store_nothing_was_written_to_is_closed_by_deleting_its_directory() {
    let mut rig = Rig::without_team_row();
    let dir = rig.team_dir();
    assert!(dir.join("team.db").is_file());

    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named());
    assert!(plan.refusals.is_empty(), "{plan:?}");
    let deleted = format!("no team or member was recorded; deleted {}", dir.display());
    let would = format!(
        "no team or member was recorded; would delete {}",
        dir.display()
    );
    let step = |s: &teamwire_close::CloseStep| (s.step.clone(), s.detail.clone());
    assert_eq!(
        plan.steps.iter().map(step).collect::<Vec<_>>(),
        [(TEAM.to_owned(), would)]
    );
    assert!(dir.join("team.db").is_file(), "a dry run changes nothing");

    let report = close(&mut rig.ctx(), CloseTarget::Team, named());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    let steps: Vec<_> = report
        .steps
        .iter()
        .map(|s| (s.step.clone(), s.detail.clone()))
        .collect();
    assert_eq!(steps, [(TEAM.to_owned(), deleted)]);
    assert!(report.archive.is_none());
    assert!(!dir.exists());
}

#[test]
fn a_member_row_without_a_team_row_is_refused_and_kept() {
    let mut rig = Rig::without_team_row();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let dir = rig.team_dir();

    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named());
    assert!(
        plan.refusals.iter().any(|r| r.contains("no team row")),
        "{plan:?}"
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, named());
    assert!(
        report.refusals.iter().any(|r| r.contains("no team row")),
        "{report:?}"
    );
    assert!(dir.join("team.db").is_file());
}

#[test]
fn an_unnamed_team_is_not_guessed_from_an_empty_store() {
    let mut rig = Rig::without_team_row();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(!report.refusals.is_empty(), "{report:?}");
    assert!(rig.team_dir().exists());
}

#[test]
fn a_message_row_without_a_team_row_is_refused_and_kept() {
    let mut rig = Rig::without_team_row();
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.insert_message(
                now,
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Finding,
                    re: None,
                    commit: None,
                    body: "kept".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Lead],
                },
            )
        })
        .unwrap();
    let dir = rig.team_dir();

    let plan = plan_close_with(&rig.ctx(), CloseTarget::Team, &named());
    assert!(
        plan.refusals.iter().any(|r| r.contains("no team row")),
        "{plan:?}"
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, named());
    assert!(
        report.refusals.iter().any(|r| r.contains("no team row")),
        "{report:?}"
    );
    assert!(dir.join("team.db").is_file());
}

#[test]
fn an_event_row_alone_does_not_stop_the_deletion() {
    let mut rig = Rig::without_team_row();
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.record_event(
                now,
                EventType::CloseRefused,
                None,
                None,
                &serde_json::json!({}),
            )
        })
        .unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, named());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    assert!(!rig.team_dir().exists());
}
