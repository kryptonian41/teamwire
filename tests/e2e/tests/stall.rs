//! `fake-herdr` stalls while a message is typed: the sender exits 5, the
//! state is `unconfirmed`, and nothing is sent a second time.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::World;
use teamwire_fakes::{ClaudeScript, HerdrFaults};

/// The `agent prompt` calls whose text holds `needle`. Delivery types into
/// the member's place (a pane id), not its name.
fn prompts_with(w: &World, needle: &str) -> usize {
    w.herdr
        .calls()
        .iter()
        .filter(|c| c.command() == "agent prompt" && c.argv.iter().any(|a| a.contains(needle)))
        .count()
}

#[test]
fn a_stalled_message_is_unconfirmed_and_never_resent() {
    let mut w = World::new(&ClaudeScript::default());
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look around"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["a"]);
    // Let the launch task finish its turn before the fault is set.
    w.wait_member_turn_done("demo", "a");

    w.herdr
        .set_faults(&HerdrFaults::new().stalled_prompt(None, None))
        .unwrap();
    let send = w.teamwire(
        &[
            "send",
            "demo",
            "--from",
            "lead",
            "--to",
            "a",
            "--type",
            "note",
            "--text",
            "are you there",
        ],
        None,
    );
    assert_eq!(send.code, 5, "{}", send.all());
    assert!(send.out.contains("unconfirmed"), "{}", send.all());

    let log = w.teamwire(&["log", "demo"], None);
    assert!(
        log.out.contains("lead -> a") && log.out.contains("unconfirmed"),
        "{}",
        log.all()
    );
    // Nothing retries after `send` exits: no watch runs and `send` never
    // forks. A short wait still lets a stray late prompt show up in the log.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        prompts_with(&w, "are you there"),
        1,
        "the message was typed once and never again"
    );
    let status = w.teamwire(&["status", "demo"], None);
    assert!(status.out.contains("unconfirmed"), "{}", status.all());
}
