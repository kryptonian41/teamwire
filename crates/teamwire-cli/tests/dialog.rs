//! `teamwire dialog show` and `teamwire dialog answer` through the real
//! binary (ADR 0052), on a team launched against `fake-herdr` and
//! `fake-claude`. Every test proves that neither command types into a pane
//! or sends a message: no `agent send-keys` and no `agent prompt` call after
//! launch, and no new `message.sent` row.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and git in a throwaway repository"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "helper functions in an integration-test file unwrap, index and panic; a failure there fails the test"
)]

#[path = "support/amend_rig.rs"]
pub mod amend_rig;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use amend_rig::{Rig, Run, role};
use serde_json::{Value, json};
use teamwire_core::{
    DialogState, EventType, HarnessId, HostId, MemberKind, MemberState, Mode, PermissionRequest,
    Timestamp, Ulid,
};
use teamwire_fakes::{ClaudeScript, PLAIN_TRUST_TEXT, Step, TRUST_OPTION_TEXT};
use teamwire_store::NewMember;

/// A caller's name, its environment, and whether its refusal gets past
/// `refuse_member` and so is recorded as `message.refused`.
type Caller<'a> = (&'a str, &'a [(&'a str, &'a str)], bool);

const LEAD: [(&str, &str); 1] = [("CLAUDE_CODE_SESSION_ID", "lead-1")];

fn now() -> Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    Timestamp::from_unix_millis(i64::try_from(millis).unwrap()).unwrap()
}

/// What a test compares against once launch and the lead's adopt are done:
/// the herdr call count and the `message.sent` row count.
struct Mark {
    calls: usize,
    sent: usize,
}

/// A launched team whose recorded lead session is `lead-1`.
fn launched(rig: &Rig, status: &str) -> Mark {
    rig.launch_until("true", &["src/"], &[], status);
    let adopt = rig.teamwire_with(&["lead", "--adopt", "alpha"], None, &LEAD);
    assert_eq!(adopt.code, 0, "{}", adopt.all());
    Mark {
        calls: rig.herdr.calls().len(),
        sent: count(rig, EventType::MessageSent),
    }
}

/// A rig whose member stops at a permission dialog for `command` in its
/// first turn, so its real permission hook runs and waits.
fn at_dialog(command: &str) -> (Rig, Mark) {
    let script = ClaudeScript {
        turns: vec![vec![Step::Permission {
            tool: "Bash".to_owned(),
            input: json!({"command": command, "description": "clean up"}),
            timeout_ms: Some(90_000),
        }]],
        ..ClaudeScript::default()
    };
    let rig = Rig::with_script(&script);
    let from = launched(&rig, "blocked");
    (rig, from)
}

fn answer(rig: &Rig, id: &str, verdict: &str, reason: &str, env: &[(&str, &str)]) -> Run {
    rig.teamwire_with(
        &[
            "dialog", "answer", "alpha", "--role", "api", "--id", id, verdict, "--reason", reason,
        ],
        None,
        env,
    )
}

fn count(rig: &Rig, kind: EventType) -> usize {
    rig.store()
        .read(|tx| tx.events_of_type(kind)?.all_or_corrupt())
        .unwrap()
        .len()
}

/// Since `from`, nothing typed into a pane and nothing sent a message.
fn typed_nothing(rig: &Rig, from: &Mark) {
    let calls = rig.calls_since(from.calls);
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("agent send-keys") || c.starts_with("agent prompt")),
        "{calls:?}"
    );
    assert_eq!(
        count(rig, EventType::MessageSent),
        from.sent,
        "a dialog command sent a message"
    );
}

/// The pending dialog the member's hook recorded, once it is there.
fn hook_dialog(rig: &Rig) -> Ulid {
    for _ in 0..500 {
        let pending = rig
            .store()
            .read(|tx| tx.dialogs_of_role(&role("api"), now()))
            .unwrap()
            .into_iter()
            .find(|d| d.state == DialogState::Pending);
        if let Some(d) = pending {
            return d.id;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the member's hook never recorded a dialog");
}

/// A dialog of `api`'s current session written through the store, as its
/// hook would, expiring `expires_in` from now (negative: already expired).
fn write_dialog(
    rig: &Rig,
    tool: &str,
    input: Value,
    why_not: Option<&str>,
    expires_in: i64,
) -> Ulid {
    let member = rig.member("api");
    let request = PermissionRequest {
        session: member.session_id.clone().unwrap(),
        subagent: false,
        tool: tool.to_owned(),
        summary: input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or(tool)
            .to_owned(),
        input: Some(input),
        why_not: why_not.map(str::to_owned),
    };
    let opened = now();
    let expires = Timestamp::from_unix_millis(opened.unix_millis() + expires_in * 1000).unwrap();
    let opened = if expires_in < 0 {
        Timestamp::from_unix_millis(expires.unix_millis() - 100_000).unwrap()
    } else {
        opened
    };
    rig.store()
        .write(|tx| tx.open_dialog(opened, &role("api"), member.generation, &request, expires))
        .unwrap()
}

fn state_of(rig: &Rig, id: &Ulid) -> DialogState {
    rig.store()
        .read(|tx| tx.dialog(id, now()))
        .unwrap()
        .unwrap()
        .state
}

#[test]
fn the_lead_approves_and_the_hook_prints_allow_with_no_key_typed() {
    let (rig, from) = at_dialog("rmdir /usr");
    let id = hook_dialog(&rig);
    let run = answer(
        &rig,
        &id.to_string(),
        "--approve",
        "it is in the task",
        &LEAD,
    );
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out
            .contains("approve (taken by the member's permission hook)"),
        "{}",
        run.out
    );
    rig.wait_for_status("done");
    let session = rig.member("api").session_id.unwrap();
    let outcomes = rig.herdr.permission_outcomes(session.as_str());
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    assert_eq!(outcomes[0]["by"], "hook");
    assert_eq!(outcomes[0]["decision"], "allow");
    assert_eq!(state_of(&rig, &id), DialogState::Taken);
    typed_nothing(&rig, &from);
}

#[test]
fn the_lead_denies_and_the_hook_reply_quotes_the_call_and_the_reason() {
    let (rig, from) = at_dialog("rm -rf target");
    let id = hook_dialog(&rig);
    let reason = "this removes all of target/; remove only target/debug/foo, by name";
    let run = answer(&rig, &id.to_string(), "--deny", reason, &LEAD);
    assert_eq!(run.code, 0, "{}", run.all());
    rig.wait_for_status("done");
    let session = rig.member("api").session_id.unwrap();
    let outcomes = rig.herdr.permission_outcomes(session.as_str());
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    assert_eq!(outcomes[0]["by"], "hook");
    assert_eq!(outcomes[0]["decision"], "deny");
    let message = outcomes[0]["message"].as_str().unwrap();
    assert!(message.starts_with("DIALOG DENIED:"), "{message}");
    assert!(message.contains("rm -rf target"), "{message}");
    assert!(message.contains(reason), "{message}");
    assert!(
        message.contains("Do not run this command another way"),
        "{message}"
    );
    typed_nothing(&rig, &from);
}

#[test]
fn an_answer_to_a_dialog_that_changed_types_nothing_and_answers_nothing() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    let old = write_dialog(&rig, "Bash", json!({"command": "rmdir /usr"}), None, -1);
    let new = write_dialog(&rig, "Bash", json!({"command": "rmdir /opt"}), None, 60);
    let run = answer(&rig, &old.to_string(), "--approve", "ok", &LEAD);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(run.err.contains("refused.expired"), "{}", run.err);
    assert_eq!(state_of(&rig, &new), DialogState::Pending);
    assert_eq!(count(&rig, EventType::DialogAnswered), 0);

    // A dialog of an earlier session of the member is refused too.
    rig.lose_process();
    let run = answer(&rig, &new.to_string(), "--approve", "ok", &LEAD);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(run.err.contains("refused.changed"), "{}", run.err);
    assert_eq!(count(&rig, EventType::DialogAnswered), 0);
    typed_nothing(&rig, &from);
}

/// A rig whose member prints `screen` in its first turn and then works for a
/// minute, so the screen shows a dialog that no permission hook recorded.
fn drawing(screen: &str) -> (Rig, Mark) {
    let script = ClaudeScript {
        turns: vec![vec![Step::Print(screen.to_owned()), Step::SleepMs(60_000)]],
        ..ClaudeScript::default()
    };
    let rig = Rig::with_script(&script);
    let from = launched(&rig, "working");
    (rig, from)
}

fn show(rig: &Rig) -> Run {
    let run = rig.teamwire_with(&["dialog", "show", "alpha", "--role", "api"], None, &LEAD);
    assert_eq!(run.code, 0, "{}", run.all());
    run
}

#[test]
fn an_answer_to_a_trust_dialog_is_refused() {
    // Claude Code runs no permission hook for a trust dialog, so no dialog
    // is ever recorded for it: any id the lead names is not one of its.
    let trust = format!("{PLAIN_TRUST_TEXT}\n\u{276f} No, exit\n  {TRUST_OPTION_TEXT}");
    let (rig, from) = drawing(&trust);
    let shown = show(&rig);
    assert!(
        shown
            .out
            .starts_with("api (claude-code on herdr): at a trust dialog\n"),
        "{}",
        shown.out
    );
    let made_up = Ulid::parse("01JDQ8M3W4X5Y6Z7A8B9C0D1E2").unwrap();
    let run = answer(&rig, &made_up.to_string(), "--approve", "ok", &LEAD);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(run.err.contains("refused.no_dialog"), "{}", run.err);
    assert_eq!(count(&rig, EventType::DialogAnswered), 0);
    typed_nothing(&rig, &from);
}

#[test]
fn a_member_cannot_answer_even_with_its_token_unset() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    let id = write_dialog(&rig, "Bash", json!({"command": "rmdir /usr"}), None, 60);
    rig.store()
        .write(|tx| tx.set_member_token_hash(&role("api"), &teamwire_store::token_hash("tok")))
        .unwrap();
    // (name, environment, whether the refusal gets past `refuse_member` and
    // so is recorded as message.refused)
    let callers: [Caller<'_>; 4] = [
        (
            "member",
            &[
                ("TEAMWIRE_TEAM", "alpha"),
                ("TEAMWIRE_ROLE", "api"),
                ("TEAMWIRE_TOKEN", "tok"),
            ],
            false,
        ),
        (
            "member with its token unset",
            &[("TEAMWIRE_TEAM", "alpha"), ("TEAMWIRE_ROLE", "api")],
            false,
        ),
        ("no identity", &[], true),
        (
            "another session",
            &[("CLAUDE_CODE_SESSION_ID", "not-the-lead")],
            true,
        ),
    ];
    for (name, env, logged) in callers {
        let refused = count(&rig, EventType::MessageRefused);
        let run = answer(&rig, &id.to_string(), "--approve", "ok", env);
        assert_eq!(run.code, 1, "{name}: {}", run.all());
        assert!(run.err.contains("refused.identity"), "{name}: {}", run.err);
        assert_eq!(
            count(&rig, EventType::MessageRefused),
            refused + usize::from(logged),
            "{name}"
        );
    }
    assert_eq!(count(&rig, EventType::DialogAnswered), 0);
    assert_eq!(state_of(&rig, &id), DialogState::Pending);
    typed_nothing(&rig, &from);
}

#[test]
fn an_unanswerable_call_is_refused() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    let other = write_dialog(
        &rig,
        "AskUserQuestion",
        json!({"questions": []}),
        Some("teamwire answers only Bash, Write, Edit and WebFetch calls"),
        60,
    );
    let big = write_dialog(
        &rig,
        "Bash",
        json!({"command": "echo"}),
        Some("the input is 70000 bytes, over the 64 KiB teamwire shows whole"),
        60,
    );
    for (id, says) in [
        (other, "only Bash, Write, Edit and WebFetch"),
        (big, "64 KiB"),
    ] {
        let run = answer(&rig, &id.to_string(), "--approve", "ok", &LEAD);
        assert_eq!(run.code, 1, "{}", run.all());
        assert!(run.err.contains("refused.not_answerable"), "{}", run.err);
        assert!(run.err.contains(says), "{}", run.err);
    }
    assert_eq!(count(&rig, EventType::DialogAnswered), 0);
    typed_nothing(&rig, &from);
}

#[test]
fn an_answer_no_hook_takes_exits_7() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    let id = write_dialog(&rig, "Bash", json!({"command": "rmdir /usr"}), None, 60);
    let run = answer(&rig, &id.to_string(), "--deny", "not that folder", &LEAD);
    assert_eq!(run.code, 7, "{}", run.all());
    assert!(
        run.out.contains("deny (recorded, not taken)"),
        "{}",
        run.out
    );
    assert!(
        run.err
            .contains("not taken: the member's permission hook did not take the answer"),
        "{}",
        run.err
    );
    assert_eq!(state_of(&rig, &id), DialogState::Answered);
    assert_eq!(count(&rig, EventType::DialogAnswered), 1);
    typed_nothing(&rig, &from);
}

#[test]
fn a_codex_member_is_refused_with_refused_harness() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    let at = now();
    rig.store()
        .write(|tx| {
            tx.insert_member(
                at,
                &NewMember {
                    role: role("cx"),
                    title: "codex work".to_owned(),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("codex"),
                    host: HostId::from_static("herdr"),
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
            tx.set_member_state(at, &role("cx"), MemberState::Started, &json!({}))
        })
        .unwrap();
    let run = rig.teamwire_with(
        &[
            "dialog",
            "answer",
            "alpha",
            "--role",
            "cx",
            "--id",
            "01JDQ8M3W4X5Y6Z7A8B9C0D1E2",
            "--approve",
            "--reason",
            "ok",
        ],
        None,
        &LEAD,
    );
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err
            .contains("refused.harness: harness codex cannot answer a dialog"),
        "{}",
        run.err
    );
    typed_nothing(&rig, &from);
}

#[test]
fn show_prints_the_command_and_types_nothing() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    let id = write_dialog(
        &rig,
        "Bash",
        json!({"command": "rmdir /usr\nrmdir /opt", "description": "clean"}),
        None,
        60,
    );
    let events = count(&rig, EventType::DialogAnswered) + count(&rig, EventType::DialogClosed);
    let run = rig.teamwire_with(&["dialog", "show", "alpha", "--role", "api"], None, &LEAD);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out.starts_with("api (claude-code on herdr): "),
        "{}",
        run.out
    );
    assert!(
        run.out
            .contains(&format!("dialog {id}: pending, answer before ")),
        "{}",
        run.out
    );
    assert!(
        run.out
            .contains("  command:\n    rmdir /usr\n    rmdir /opt\n"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains(&format!(
            "  answer: teamwire dialog answer alpha --role api --id {id} --approve|--deny --reason TEXT"
        )),
        "{}",
        run.out
    );
    assert!(run.out.contains("screen (last 60 lines):"), "{}", run.out);

    let json = rig.teamwire_with(
        &["dialog", "show", "alpha", "--role", "api", "--json"],
        None,
        &LEAD,
    );
    assert_eq!(json.code, 0, "{}", json.all());
    let doc: Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(doc["dialogs"][0]["id"], id.to_string());
    assert_eq!(doc["dialogs"][0]["state"], "pending");
    assert_eq!(
        doc["dialogs"][0]["input"]["command"],
        "rmdir /usr\nrmdir /opt"
    );

    // A member session cannot read it.
    let member = rig.teamwire_with(
        &["dialog", "show", "alpha", "--role", "api"],
        None,
        &[("TEAMWIRE_ROLE", "api")],
    );
    assert_eq!(member.code, 1, "{}", member.all());
    assert!(member.err.contains("refused.identity"), "{}", member.err);

    assert_eq!(state_of(&rig, &id), DialogState::Pending);
    assert_eq!(
        count(&rig, EventType::DialogAnswered) + count(&rig, EventType::DialogClosed),
        events
    );
    typed_nothing(&rig, &from);
}

#[test]
fn show_escapes_control_and_invisible_characters_in_the_command() {
    let rig = Rig::new();
    let from = launched(&rig, "done");
    write_dialog(
        &rig,
        "Bash",
        json!({"command": "rm a\u{1b}[2Jb\u{202e}c\u{200b}d"}),
        None,
        60,
    );
    let run = rig.teamwire_with(&["dialog", "show", "alpha", "--role", "api"], None, &LEAD);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out
            .contains("    rm a\\u{1b}[2Jb\\u{202e}c\\u{200b}d\n"),
        "{}",
        run.out
    );
    for hidden in ['\u{1b}', '\u{202e}', '\u{200b}'] {
        assert!(!run.out.contains(hidden), "{hidden:?} printed raw");
    }
    typed_nothing(&rig, &from);
}
