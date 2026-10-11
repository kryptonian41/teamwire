//! Removing an implementer without a verify record is refused; after a
//! verify that passed at its head, the removal is accepted.

use std::time::Duration;

use serde_json::json;
use teamwire_core::{MemberState, Role};
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn removing_an_implementer_needs_a_verify_first() {
    let step = format!(
        "{}{}{}{}",
        sh::commit(),
        sh::report(true),
        sh::done("built it"),
        ""
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));
    let worktree = w.repo().join(".worktrees/a");

    let refused = w.teamwire(&["close", "demo", "--role", "a"], None);
    assert_eq!(refused.code, 1, "{}", refused.all());
    assert!(refused.err.contains("verify"), "{}", refused.all());
    assert!(worktree.is_dir(), "a refusal changes nothing");
    let state = || {
        w.store("demo")
            .read(|tx| tx.member(&Role::parse("a").unwrap()))
            .unwrap()
            .unwrap()
            .state
    };
    assert_ne!(state(), MemberState::Removed);

    let verify = w.teamwire(&["verify", "demo", "--role", "a"], None);
    assert_eq!(verify.code, 0, "{}", verify.all());
    let accepted = w.teamwire(&["close", "demo", "--role", "a"], None);
    assert_eq!(accepted.code, 0, "{}", accepted.all());
    assert!(!worktree.exists(), "the worktree is removed");
    assert_eq!(state(), MemberState::Removed);
    let branches = w.git(w.repo(), &["branch", "--list"]);
    assert!(
        branches.lines().any(|l| l.trim() == "a"),
        "the branch is kept: {branches}"
    );
}
