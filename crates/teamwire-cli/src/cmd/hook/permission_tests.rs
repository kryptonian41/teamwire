//! The permission hook runs in-process against the real Claude Code harness,
//! with the wait shortened through its argument.
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{
    Clock as _, DialogState, DialogVerdict, EventType, HarnessId, HostId, LeadRoute, MemberKind,
    MemberState, Mode, PermissionAnswer, Role, SessionId, SessionKind, TeamName,
};
use teamwire_fakes::TestHome;
use teamwire_store::{AnswerOutcome, DialogRecord, NewMember, NewTeam, Store};

use super::permission_hook_in;
use crate::cmd::hook::member_tests::real_wiring;
use crate::wiring::Wiring;

const TOKEN: &str = "member-token";
const SESSION: &str = "f07ea8cf-a9c1-41d3-9162-d68dbdefef83";

struct Bed {
    home: TestHome,
    team: TeamName,
    role: Role,
    wiring: Wiring,
}

/// A team whose only member `api` runs on Claude Code, with a token and the
/// session `SESSION`.
fn bed() -> Bed {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("alpha").unwrap();
    let role = Role::parse("api").unwrap();
    let wiring = real_wiring();
    let now = wiring.clock.now();
    let mut store = Store::open(home.path(), &team).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                now,
                &NewTeam {
                    name: team.clone(),
                    title: "Alpha".to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("none"),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((
                        SessionId::parse("lead-1").unwrap(),
                        SessionKind::Interactive,
                    )),
                },
            )?;
            tx.insert_member(
                now,
                &NewMember {
                    role: role.clone(),
                    title: "the api".to_owned(),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("claude-code"),
                    host: HostId::from_static("none"),
                    cwd: std::env::temp_dir(),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: Vec::new(),
                    deadline_s: 1800,
                },
            )?;
            tx.set_member_state(now, &role, MemberState::Started, &json!({}))?;
            tx.set_member_token_hash(&role, &teamwire_msg::token_hash(TOKEN))?;
            tx.set_member_session(&role, &SessionId::parse(SESSION).unwrap())
        })
        .unwrap();
    Bed {
        home,
        team,
        role,
        wiring,
    }
}

/// Claude Code's hook input for `tool` with `input`, from `session`.
fn stdin(session: &str, tool: &str, input: &Value) -> String {
    json!({
        "session_id": session,
        "transcript_path": "/work/t.jsonl",
        "cwd": "/work",
        "permission_mode": "bypassPermissions",
        "hook_event_name": "PermissionRequest",
        "tool_name": tool,
        "tool_input": input,
        "permission_suggestions": [],
    })
    .to_string()
}

fn rmdir() -> String {
    stdin(SESSION, "Bash", &json!({"command": "rmdir /usr"}))
}

fn run(bed: &Bed, token: Option<&str>, input: &str, wait: Duration) -> Option<Vec<u8>> {
    permission_hook_in(
        bed.home.path(),
        &bed.team,
        &bed.role,
        token,
        &bed.wiring,
        &mut input.as_bytes(),
        wait,
    )
}

fn dialogs(bed: &Bed) -> Vec<DialogRecord> {
    let store = Store::open_existing(bed.home.path(), &bed.team).unwrap();
    store
        .read(|tx| tx.dialogs_of_role(&bed.role, bed.wiring.clock.now()))
        .unwrap()
}

fn rows_of(bed: &Bed, kind: EventType) -> usize {
    let store = Store::open_existing(bed.home.path(), &bed.team).unwrap();
    store
        .read(|tx| tx.events_of_role(&bed.role)?.all_or_corrupt())
        .unwrap()
        .iter()
        .filter(|e| e.kind == kind)
        .count()
}

#[test]
fn the_hook_prints_nothing_when_nobody_answers() {
    let bed = bed();
    let reply = run(&bed, Some(TOKEN), &rmdir(), Duration::from_millis(300));
    assert_eq!(reply, None);
    let all = dialogs(&bed);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].state, DialogState::Expired);
    assert_eq!(all[0].request.summary, "rmdir /usr");
    assert_eq!(rows_of(&bed, EventType::DialogClosed), 1);
}

#[test]
fn the_hook_prints_the_answer_it_took() {
    let bed = bed();
    let (home, team, role) = (
        bed.home.path().to_owned(),
        bed.team.clone(),
        bed.role.clone(),
    );
    // The lead answers from another thread once the dialog is recorded.
    let lead = std::thread::spawn(move || {
        let clock = real_wiring().clock;
        for _ in 0..400 {
            let mut store = Store::open_existing(&home, &team).unwrap();
            let pending = store
                .read(|tx| tx.dialogs_of_role(&role, clock.now()))
                .unwrap()
                .into_iter()
                .find(|d| d.state == DialogState::Pending);
            if let Some(dialog) = pending {
                let answer = PermissionAnswer {
                    verdict: DialogVerdict::Deny,
                    reason: "remove only target/debug/foo".to_owned(),
                };
                return store
                    .write(|tx| tx.answer_dialog(clock.now(), &role, &dialog.id, &answer))
                    .unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        AnswerOutcome::NoDialog
    });
    let reply = run(&bed, Some(TOKEN), &rmdir(), Duration::from_secs(20));
    assert!(matches!(lead.join().unwrap(), AnswerOutcome::Recorded(_)));
    let reply: Value = serde_json::from_slice(&reply.expect("a reply")).unwrap();
    let decision = &reply["hookSpecificOutput"]["decision"];
    assert_eq!(decision["behavior"], "deny");
    let message = decision["message"].as_str().unwrap();
    assert!(message.starts_with("DIALOG DENIED:"), "{message}");
    assert!(message.contains("rmdir /usr"), "{message}");
    assert!(
        message.contains("remove only target/debug/foo"),
        "{message}"
    );
    assert_eq!(dialogs(&bed)[0].state, DialogState::Taken);
}

#[test]
fn a_wrong_token_or_session_records_nothing() {
    let bed = bed();
    let wait = Duration::from_millis(300);
    assert_eq!(run(&bed, Some("not-the-token"), &rmdir(), wait), None);
    assert_eq!(run(&bed, None, &rmdir(), wait), None);
    let other = stdin(
        "0b7a1d3e-0000-4000-8000-000000000000",
        "Bash",
        &json!({"command": "rmdir /usr"}),
    );
    assert_eq!(run(&bed, Some(TOKEN), &other, wait), None);
    assert_eq!(rows_of(&bed, EventType::DialogOpened), 0);
}

#[test]
fn an_unanswerable_call_prints_nothing_and_is_closed() {
    let bed = bed();
    let input = stdin(SESSION, "AskUserQuestion", &json!({"questions": []}));
    let clock = real_wiring().clock;
    let start = clock.monotonic();
    // A long wait: the hook must not spend it on a call it will not answer.
    let reply = run(&bed, Some(TOKEN), &input, Duration::from_secs(30));
    assert_eq!(reply, None);
    assert!(clock.monotonic() - start < Duration::from_secs(5));
    let all = dialogs(&bed);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].state, DialogState::Unanswerable);
}

/// `input` with the key `key` taken out.
fn without(key: &str) -> String {
    let mut doc: Value = serde_json::from_str(&rmdir()).unwrap();
    doc.as_object_mut().unwrap().remove(key);
    doc.to_string()
}

#[test]
fn a_bad_hook_input_prints_nothing_and_records_nothing() {
    let bed = bed();
    let wait = Duration::from_millis(300);
    let other_session = stdin(
        "0b7a1d3e-0000-4000-8000-000000000000",
        "Bash",
        &json!({"command": "rmdir /usr"}),
    );
    // Over the size limit: production reads through `take(STDIN_LIMIT)`, so
    // the hook sees the input cut at the limit, in the middle of the command.
    let huge = stdin(SESSION, "Bash", &json!({"command": "x".repeat(2 << 20)}));
    let limit = usize::try_from(super::STDIN_LIMIT).unwrap();
    let cut = &huge.as_bytes()[..limit];
    let cases: [(&str, Vec<u8>); 6] = [
        ("malformed JSON", b"{\"session_id\": ".to_vec()),
        ("no tool_input", without("tool_input").into_bytes()),
        (
            "no hook_event_name",
            without("hook_event_name").into_bytes(),
        ),
        ("no session_id", without("session_id").into_bytes()),
        ("over the size limit", cut.to_vec()),
        ("another session", other_session.into_bytes()),
    ];
    for (name, input) in cases {
        let reply = permission_hook_in(
            bed.home.path(),
            &bed.team,
            &bed.role,
            Some(TOKEN),
            &bed.wiring,
            &mut &input[..],
            wait,
        );
        assert_eq!(reply, None, "{name}");
    }
    assert_eq!(rows_of(&bed, EventType::DialogOpened), 0);
}
