//! A member at a permission dialog, end to end (ADR 0052): fake-claude runs
//! the real `PermissionRequest` hook from the settings the Claude harness
//! wrote, the watch prints the dialog's line, and the lead's `teamwire
//! dialog answer` reaches the member only through that hook. Nothing is
//! typed into the member's pane after launch.
#![expect(
    clippy::disallowed_methods,
    reason = "the test reads the wall clock to read a dialog's state at the moment it runs"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use teamwire_core::{DialogState, EventType, Role, Timestamp, Ulid};
use teamwire_e2e::{World, wait_until};
use teamwire_fakes::{ClaudeScript, Step};

fn now() -> Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    Timestamp::from_unix_millis(i64::try_from(millis).unwrap()).unwrap()
}

/// A member whose first turn stops at a permission dialog for `command`.
fn script(command: &str) -> ClaudeScript {
    ClaudeScript {
        turns: vec![vec![Step::Permission {
            tool: "Bash".to_owned(),
            input: json!({"command": command, "description": "clean up"}),
            timeout_ms: Some(90_000),
        }]],
        ..ClaudeScript::default()
    }
}

fn spec(w: &World) -> Value {
    json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]}]})
}

/// Launches the team, then waits for the member's hook to record its dialog.
/// Returns the dialog id and the herdr call count once launch is done.
fn at_dialog(w: &World) -> (Ulid, usize) {
    let launch = w.launch(&spec(w), &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    let from = w.herdr.calls().len();
    let mut id = None;
    let found = wait_until(Duration::from_secs(30), || {
        id = w
            .store("demo")
            .read(|tx| tx.dialogs_of_role(&Role::parse("a").unwrap(), now()))
            .unwrap()
            .into_iter()
            .find(|d| d.state == DialogState::Pending)
            .map(|d| d.id);
        id.is_some()
    });
    assert!(found, "the member's hook never recorded a dialog");
    (id.unwrap(), from)
}

/// No key typed into any pane, and no prompt typed into the member's pane,
/// after call `from`.
fn typed_nothing(w: &World, from: usize) {
    let calls: Vec<String> = w.herdr.calls()[from..]
        .iter()
        .map(|c| c.argv.join(" "))
        .collect();
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("agent send-keys") || c.starts_with("agent prompt tw_demo_a")),
        "{calls:?}"
    );
}

fn sent(w: &World) -> usize {
    w.store("demo")
        .read(|tx| tx.events_of_type(EventType::MessageSent)?.all_or_corrupt())
        .unwrap()
        .len()
}

/// The outcome fake-claude wrote for the member's one dialog.
fn outcome(w: &World) -> Value {
    let session = w
        .store("demo")
        .read(|tx| tx.member(&Role::parse("a").unwrap()))
        .unwrap()
        .unwrap()
        .session_id
        .unwrap();
    let mut got = Vec::new();
    let found = wait_until(Duration::from_secs(30), || {
        got = w.herdr.permission_outcomes(session.as_str());
        !got.is_empty()
    });
    assert!(found, "the dialog never ended");
    assert_eq!(got.len(), 1, "{got:?}");
    got.remove(0)
}

#[test]
fn a_member_at_a_permission_dialog_is_approved_through_its_hook() {
    let w = World::new(&script("rmdir /usr"));
    let (id, from) = at_dialog(&w);
    let watch = w.teamwire(&["watch", "demo", "--once"], None);
    assert!(
        watch.out.contains(&format!(
            "BLOCKED a permission Bash: rmdir /usr (dialog {id}, answer by "
        )),
        "{}",
        watch.all()
    );
    let answer = w.teamwire(
        &[
            "dialog",
            "answer",
            "demo",
            "--role",
            "a",
            "--id",
            &id.to_string(),
            "--approve",
            "--reason",
            "the task asks for it",
        ],
        None,
    );
    assert_eq!(answer.code, 0, "{}", answer.all());
    let got = outcome(&w);
    assert_eq!(got["by"], "hook");
    assert_eq!(got["decision"], "allow");
    typed_nothing(&w, from);
}

#[test]
fn a_denied_member_reads_the_call_and_the_reason_in_its_tool_result() {
    let w = World::new(&script("rm -rf target"));
    let (id, from) = at_dialog(&w);
    let before = sent(&w);
    let reason = "this removes all of target/; remove only target/debug/foo, by name";
    let answer = w.teamwire(
        &[
            "dialog",
            "answer",
            "demo",
            "--role",
            "a",
            "--id",
            &id.to_string(),
            "--deny",
            "--reason",
            reason,
        ],
        None,
    );
    assert_eq!(answer.code, 0, "{}", answer.all());
    let got = outcome(&w);
    assert_eq!(got["by"], "hook");
    assert_eq!(got["decision"], "deny");
    let message = got["message"].as_str().unwrap();
    assert!(message.starts_with("DIALOG DENIED:"), "{message}");
    assert!(message.contains("rm -rf target"), "{message}");
    assert!(message.contains(reason), "{message}");
    let shown = wait_until(Duration::from_secs(10), || {
        let screen = w.member_screen("demo", "a");
        screen.contains("Denied by PermissionRequest hook") && screen.contains("DIALOG DENIED:")
    });
    assert!(shown, "{}", w.member_screen("demo", "a"));
    typed_nothing(&w, from);
    assert_eq!(sent(&w), before);
}
