//! The permission-dialog rules in the store (ADR 0052): the dialog id is
//! the proof, an answer is recorded once and only while the dialog is
//! pending, and a dialog whose hook died never stays pending.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

use std::path::PathBuf;

use serde_json::json;
use teamwire_core::{
    DialogState, DialogVerdict, EventType, HarnessId, HostId, LeadRoute, MemberKind, MemberState,
    Mode, PermissionAnswer, PermissionRequest, Role, SessionId, SessionKind, TeamName, Timestamp,
    Ulid,
};
use teamwire_fakes::TestHome;
use teamwire_store::{AnswerOutcome, NewMember, NewTeam, Store, dialogs_from};

const SESSION: &str = "f07ea8cf-a9c1-41d3-9162-d68dbdefef83";

fn at(n: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + n).unwrap()
}

fn role() -> Role {
    Role::parse("api").unwrap()
}

/// A team with one started member `api` in `SESSION`, generation 1.
fn store(home: &TestHome) -> Store {
    let team = TeamName::parse("alpha").unwrap();
    let mut store = Store::open(home.path(), &team).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: team.clone(),
                    title: "Alpha".to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((
                        SessionId::parse("lead-1").unwrap(),
                        SessionKind::Interactive,
                    )),
                },
            )?;
            tx.insert_member(
                at(1),
                &NewMember {
                    role: role(),
                    title: "API".to_owned(),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("claude-code"),
                    host: HostId::from_static("herdr"),
                    cwd: PathBuf::from("/r"),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: Vec::new(),
                    deadline_s: 3600,
                },
            )?;
            tx.set_member_session(&role(), &SessionId::parse(SESSION).unwrap())?;
            tx.set_member_state(at(2), &role(), MemberState::Started, &json!({}))
        })
        .unwrap();
    store
}

fn request(command: &str) -> PermissionRequest {
    PermissionRequest {
        session: SessionId::parse(SESSION).unwrap(),
        subagent: false,
        tool: "Bash".to_owned(),
        summary: command.to_owned(),
        input: Some(json!({ "command": command })),
        why_not: None,
    }
}

fn deny(reason: &str) -> PermissionAnswer {
    PermissionAnswer {
        verdict: DialogVerdict::Deny,
        reason: reason.to_owned(),
    }
}

/// Opens a dialog at `at(100)` that expires at `at(100 + life)`.
fn open(store: &mut Store, req: &PermissionRequest, life: i64) -> Ulid {
    store
        .write(|tx| tx.open_dialog(at(100), &role(), 1, req, at(100 + life)))
        .unwrap()
}

fn state(store: &Store, id: &Ulid, now: i64) -> DialogState {
    store
        .read(|tx| tx.dialog(id, at(now)))
        .unwrap()
        .unwrap()
        .state
}

#[test]
fn an_answer_is_recorded_once_and_a_second_is_refused() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let id = open(&mut store, &request("rmdir /usr"), 100_000);
    assert_eq!(state(&store, &id, 200), DialogState::Pending);

    let first = store
        .write(|tx| tx.answer_dialog(at(300), &role(), &id, &deny("system folder")))
        .unwrap();
    let AnswerOutcome::Recorded(record) = first else {
        panic!("the first answer was refused: {first:?}");
    };
    assert_eq!(record.state, DialogState::Answered);
    assert_eq!(record.answer, Some(deny("system folder")));
    assert_eq!(record.answered_at, Some(at(300)));

    let second = store
        .write(|tx| {
            tx.answer_dialog(
                at(400),
                &role(),
                &id,
                &PermissionAnswer {
                    verdict: DialogVerdict::Approve,
                    reason: "changed my mind".to_owned(),
                },
            )
        })
        .unwrap();
    assert_eq!(second, AnswerOutcome::NotPending(DialogState::Answered));
    let answered = store
        .read(|tx| Ok(tx.events_of_type(EventType::DialogAnswered)?.items))
        .unwrap();
    assert_eq!(answered.len(), 1, "one answer row");
    assert_eq!(answered[0].subject, Some(id));
    assert_eq!(answered[0].data["verdict"], "deny");
    assert_eq!(answered[0].data["by"], "lead");
    assert_eq!(answered[0].data["summary"], "rmdir /usr");

    let unknown = Ulid::from_parts(at(5), 9);
    let none = store
        .write(|tx| tx.answer_dialog(at(400), &role(), &unknown, &deny("x")))
        .unwrap();
    assert_eq!(none, AnswerOutcome::NoDialog);
}

#[test]
fn the_hook_takes_an_answer_recorded_before_its_close() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let id = open(&mut store, &request("rmdir /usr"), 1_000);
    store
        .write(|tx| tx.answer_dialog(at(900), &role(), &id, &deny("no")))
        .unwrap();
    // The hook closes after the dialog's time ran out: the answer still counts.
    let taken = store.write(|tx| tx.close_dialog(at(1_500), &id)).unwrap();
    assert_eq!(taken, Some(deny("no")));
    assert_eq!(state(&store, &id, 1_600), DialogState::Taken);
    let again = store.write(|tx| tx.close_dialog(at(1_700), &id)).unwrap();
    assert_eq!(again, None, "a closed dialog closes once");
    let closed = store
        .read(|tx| Ok(tx.events_of_type(EventType::DialogClosed)?.items))
        .unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].data["outcome"], "taken");
    assert_eq!(closed[0].data["verdict"], "deny");
}

#[test]
fn an_answer_after_expiry_is_refused() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let id = open(&mut store, &request("rmdir /usr"), 1_000);
    let late = store
        .write(|tx| tx.answer_dialog(at(1_100), &role(), &id, &deny("late")))
        .unwrap();
    assert_eq!(late, AnswerOutcome::NotPending(DialogState::Expired));
    let closed = store.write(|tx| tx.close_dialog(at(1_200), &id)).unwrap();
    assert_eq!(closed, None, "nothing to take");
    assert_eq!(state(&store, &id, 1_300), DialogState::Expired);
    let refused = store
        .write(|tx| tx.answer_dialog(at(1_300), &role(), &id, &deny("later")))
        .unwrap();
    assert_eq!(refused, AnswerOutcome::NotPending(DialogState::Expired));
}

#[test]
fn an_answer_to_an_earlier_generation_is_changed() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let id = open(&mut store, &request("rmdir /usr"), 100_000);
    store
        .write(|tx| tx.bump_generation(at(200), &role(), "m001").map(|_| ()))
        .unwrap();
    let changed = store
        .write(|tx| tx.answer_dialog(at(300), &role(), &id, &deny("x")))
        .unwrap();
    assert_eq!(changed, AnswerOutcome::Changed);

    // A dialog of the right generation from another session is changed too.
    let mut other = request("rmdir /usr");
    other.session = SessionId::parse("other-session").unwrap();
    let id2 = store
        .write(|tx| tx.open_dialog(at(400), &role(), 2, &other, at(100_000)))
        .unwrap();
    let changed2 = store
        .write(|tx| tx.answer_dialog(at(500), &role(), &id2, &deny("x")))
        .unwrap();
    assert_eq!(changed2, AnswerOutcome::Changed);

    // An id of another member's dialog is no dialog of this one.
    let web = Role::parse("web").unwrap();
    store
        .write(|tx| {
            tx.insert_member(
                at(600),
                &NewMember {
                    role: web.clone(),
                    title: "Web".to_owned(),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("claude-code"),
                    host: HostId::from_static("herdr"),
                    cwd: PathBuf::from("/w"),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: Vec::new(),
                    deadline_s: 3600,
                },
            )?;
            tx.set_member_session(&web, &SessionId::parse("other-session").unwrap())?;
            tx.bump_generation(at(601), &web, "m002").map(|_| ())
        })
        .unwrap();
    let missing = store
        .write(|tx| tx.answer_dialog(at(500), &web, &id2, &deny("x")))
        .unwrap();
    assert_eq!(missing, AnswerOutcome::NoDialog);
}

#[test]
fn an_unanswerable_dialog_is_closed_at_once() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let mut req = request("AskUserQuestion");
    req.tool = "AskUserQuestion".to_owned();
    req.why_not = Some("teamwire answers only Bash, Write, Edit and WebFetch calls".to_owned());
    let id = open(&mut store, &req, 100_000);
    assert_eq!(state(&store, &id, 200), DialogState::Unanswerable);
    let closed = store.read(|tx| Ok(tx.events_about(&id)?.items)).unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].kind, EventType::DialogClosed);
    assert_eq!(closed[0].data["outcome"], "unanswerable");
    let refused = store
        .write(|tx| tx.answer_dialog(at(300), &role(), &id, &deny("x")))
        .unwrap();
    assert_eq!(
        refused,
        AnswerOutcome::NotPending(DialogState::Unanswerable)
    );
}

#[test]
fn a_dialog_past_its_time_with_no_close_reads_expired() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let id = open(&mut store, &request("rmdir /usr"), 1_000);
    assert_eq!(state(&store, &id, 1_099), DialogState::Pending);
    assert_eq!(state(&store, &id, 1_100), DialogState::Expired);
    let all = store
        .read(|tx| tx.dialogs_of_role(&role(), at(1_100)))
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].state, DialogState::Expired);
    assert_eq!(all[0].expires_at, at(1_100));
}

#[test]
fn dialogs_from_reads_state_from_the_events() {
    let home = TestHome::new().unwrap();
    let mut store = store(&home);
    let a = open(&mut store, &request("rmdir /a"), 100_000);
    let b = open(&mut store, &request("rmdir /b"), 100_000);
    store
        .write(|tx| tx.answer_dialog(at(200), &role(), &a, &deny("no")))
        .unwrap();
    let events = store
        .read(|tx| Ok(tx.events_of_role(&role())?.items))
        .unwrap();
    let found = dialogs_from(&events, at(300));
    let ids: Vec<Ulid> = found.iter().map(|d| d.id).collect();
    assert_eq!(ids, [b, a], "newest first");
    assert_eq!(found[0].state, DialogState::Pending);
    assert_eq!(found[0].request, request("rmdir /b"));
    assert_eq!(found[0].generation, 1);
    assert_eq!(found[0].role, role());
    assert_eq!(found[1].state, DialogState::Answered);
    assert_eq!(found[1].answer, Some(deny("no")));
    assert_eq!(found[1].opened_at, at(100));
    assert_eq!(found[1].closed_at, None);
    let by_role = store
        .read(|tx| tx.dialogs_of_role(&role(), at(300)))
        .unwrap();
    assert_eq!(by_role, found);
}
