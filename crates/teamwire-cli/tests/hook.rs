//! `teamwire hook claude-stop` and `claude-post-tool`, run as the real binary
//! a harness starts. A hook exits 0 whatever happens.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary as a harness would"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index JSON; a failure there fails the test"
)]

#[path = "../../teamwire-verify/tests/support/fixture.rs"]
pub mod fixture;

use std::io::Write;
use std::process::{Command, Stdio};

use fixture::{Fixture, MemberSetup};
use teamwire_core::{
    Actor, DeliveryState, MemberKind, MessageType, Mode, Recipient, Role, Route, SessionId,
};
use teamwire_store::{DeliveryUpdate, NewMessage};

struct Run {
    code: i32,
    out: String,
}

fn team() -> Fixture {
    let mut fx = Fixture::new();
    fx.add_member(&MemberSetup {
        role: "api",
        kind: MemberKind::Investigator,
        mode: Mode::Pane,
        check: None,
        owns: &[],
        base_sha: None,
        cwd: std::env::temp_dir(),
    });
    let api = Role::parse("api").unwrap();
    fx.store
        .write(|tx| tx.set_member_session(&api, &SessionId::parse("sess-1").unwrap()))
        .unwrap();
    fx
}

fn hook(fx: &Fixture, which: &str, env: &[(&str, &str)], stdin: &str) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(fx.home.path())
        .args(["hook", which, "--team", "alpha", "--role", "api"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
    }
}

fn as_api() -> [(&'static str, &'static str); 2] {
    [("TEAMWIRE_TEAM", "alpha"), ("TEAMWIRE_ROLE", "api")]
}

fn stop_input(session: &str) -> String {
    format!(r#"{{"session_id":"{session}","hook_event_name":"Stop","stop_hook_active":false}}"#)
}

fn complete(fx: &mut Fixture) {
    let api = Role::parse("api").unwrap();
    teamwire_report::done(
        &mut fx.ctx(),
        &api,
        teamwire_core::CompletionStatus::Done,
        "finished",
    )
    .unwrap();
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::last_event_seq` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn events(fx: &Fixture) -> usize {
    fx.store
        .read(|tx| tx.last_event_seq())
        .unwrap()
        .try_into()
        .unwrap()
}

fn home_entries(fx: &Fixture) -> usize {
    std::fs::read_dir(fx.home.path()).unwrap().count()
}

#[test]
fn garbage_stdin_exits_zero() {
    let fx = team();
    let before = events(&fx);
    for which in ["claude-stop", "claude-post-tool"] {
        for input in ["", "not json {{", "[1,2,3]", "{}", "\u{0}\u{1}"] {
            let run = hook(&fx, which, &as_api(), input);
            assert_eq!(run.code, 0, "{which} {input:?}");
            assert_eq!(run.out, "", "{which} {input:?}");
        }
    }
    assert_eq!(events(&fx), before, "nothing changed");
}

#[test]
fn non_member_returns_immediately() {
    let mut fx = team();
    complete(&mut fx);
    let before = events(&fx);
    let entries = home_entries(&fx);
    // No TEAMWIRE_TEAM: any session that is not a member, and the team is
    // not even looked up (the state directory here would give the answer away).
    let run = hook(&fx, "claude-stop", &[], &stop_input("sess-1"));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
    let run = hook(&fx, "claude-post-tool", &[], &stop_input("sess-1"));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
    // A variable that names another team leaves as well.
    let run = hook(
        &fx,
        "claude-stop",
        &[("TEAMWIRE_TEAM", "other"), ("TEAMWIRE_ROLE", "api")],
        &stop_input("sess-1"),
    );
    assert_eq!((run.code, run.out.as_str()), (0, ""));
    assert_eq!(events(&fx), before);
    assert_eq!(home_entries(&fx), entries, "no file was created");
}

#[test]
fn a_missing_database_exits_zero_and_creates_nothing() {
    let fx = Fixture::new();
    let ghost = [("TEAMWIRE_TEAM", "ghost"), ("TEAMWIRE_ROLE", "api")];
    let run = hook(&fx, "claude-stop", &ghost, &stop_input("sess-1"));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
    assert!(!fx.home.path().join("teams/ghost").exists());
}

#[test]
fn stop_holds_once_with_the_claude_reply_shape() {
    let mut fx = team();
    complete(&mut fx); // no report file yet
    let run = hook(&fx, "claude-stop", &as_api(), &stop_input("sess-1"));
    assert_eq!(run.code, 0);
    let reply: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(reply["hookSpecificOutput"]["hookEventName"], "Stop");
    let note = reply["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        note.contains("reports/api.md") && note.contains("does not exist"),
        "{note}"
    );
}

/// A shell task the member left running, possibly stuck for hours, must not
/// keep a recorded completion from the lead. The task shape is what Claude
/// Code 2.1.284 sends.
#[test]
fn a_running_background_task_does_not_stop_the_hook() {
    let mut fx = team();
    complete(&mut fx); // no report file yet, so the hook holds
    let input = r#"{"session_id":"sess-1","hook_event_name":"Stop","stop_hook_active":false,
        "background_tasks":[{"id":"b946yuhy9","type":"shell","status":"running",
        "description":"Start a 900-second sleep in the background","command":"sleep 900"}]}"#;
    let run = hook(&fx, "claude-stop", &as_api(), input);
    assert_eq!(run.code, 0);
    let reply: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(reply["hookSpecificOutput"]["hookEventName"], "Stop");
}

#[test]
fn a_different_session_in_the_place_is_not_the_member() {
    let mut fx = team();
    complete(&mut fx);
    let before = events(&fx);
    let run = hook(&fx, "claude-stop", &as_api(), &stop_input("someone-else"));
    assert_eq!((run.code, run.out.as_str()), (0, ""));
    assert_eq!(events(&fx), before);
}

#[test]
fn a_subagent_stop_is_ignored() {
    let mut fx = team();
    complete(&mut fx);
    let input = r#"{"session_id":"sess-1","agent_id":"sub-1","stop_hook_active":false}"#;
    let run = hook(&fx, "claude-stop", &as_api(), input);
    assert_eq!((run.code, run.out.as_str()), (0, ""));
}

#[test]
fn post_tool_reminds_once_per_unread_message() {
    let mut fx = team();
    let api = Role::parse("api").unwrap();
    let at = teamwire_core::Clock::now(&fx.clock);
    let recipient = Recipient::Member(api.clone());
    let id = fx
        .store
        .write(|tx| {
            let stored = tx.insert_message(
                at,
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: "read this".to_owned(),
                    client_key: None,
                    recipients: vec![recipient.clone()],
                },
            )?;
            tx.set_delivery_state(
                at,
                &stored.message.ulid,
                &recipient,
                &DeliveryUpdate {
                    state: DeliveryState::HandedOff,
                    route: Some(Route::HostTyping),
                    error: None,
                    attempts: 1,
                },
            )?;
            Ok(stored.message.display_id.to_string())
        })
        .unwrap();
    let input = r#"{"session_id":"sess-1","hook_event_name":"PostToolUse","tool_name":"Bash"}"#;

    let run = hook(&fx, "claude-post-tool", &as_api(), input);
    assert_eq!(run.code, 0);
    let reply: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(reply["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert!(
        reply["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains(&id)
    );

    let again = hook(&fx, "claude-post-tool", &as_api(), input);
    assert_eq!((again.code, again.out.as_str()), (0, ""), "each id once");

    let other = r#"{"session_id":"someone-else","hook_event_name":"PostToolUse"}"#;
    assert_eq!(hook(&fx, "claude-post-tool", &as_api(), other).out, "");
}
