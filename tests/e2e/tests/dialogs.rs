//! A launch whose member shows a startup dialog: the plain trust dialog is
//! answered and the member starts; the escalated one is never answered, the
//! member is refused, its pane is closed and nothing is left running.

use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::MemberState;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Startup, Step};

/// The keys typed with `agent send-keys`, one entry per call.
fn keys_sent(w: &World) -> Vec<Vec<String>> {
    w.herdr
        .calls()
        .iter()
        .filter(|c| c.command() == "agent send-keys")
        .map(|c| c.argv.iter().skip(3).cloned().collect())
        .collect()
}

fn script(startup: Startup) -> ClaudeScript {
    ClaudeScript {
        turns: vec![vec![Step::Run(format!(
            "{}{}",
            sh::report(false),
            sh::done("finished")
        ))]],
        startup,
        ..ClaudeScript::default()
    }
}

fn spec(w: &World) -> Value {
    json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]}]})
}

#[test]
fn the_plain_trust_dialog_is_answered_and_the_member_starts() {
    let w = World::new(&script(Startup::Trust));
    let launch = w.launch(&spec(&w), &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    assert!(launch.out.contains("a: started"), "{}", launch.all());
    // The member got past the dialog: its task ran and its result arrived.
    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));
    let member = w.state_of("demo", "a");
    assert_eq!(member, MemberState::Started);
    // Only `down` then `enter` trusts the folder: a bare `enter` would
    // have taken "No, exit".
    assert_eq!(
        keys_sent(&w),
        vec![vec!["down".to_owned()], vec!["enter".to_owned()]]
    );
    assert!(w.repo().join(".worktrees/a").is_dir());
}

#[test]
fn the_escalated_trust_dialog_is_refused_and_everything_is_cleaned_up() {
    let w = World::new(&script(Startup::EscalatedTrust));
    let launch = w.launch(&spec(&w), &[]);
    assert_eq!(launch.code, 2, "{}", launch.all());
    assert!(launch.out.contains("a: failed:"), "{}", launch.all());
    assert!(
        launch.out.contains("0/1 members started"),
        "{}",
        launch.all()
    );
    assert_eq!(
        keys_sent(&w),
        Vec::<Vec<String>>::new(),
        "no key was ever typed into the dialog"
    );
    assert!(
        !w.repo().join(".worktrees/a").exists(),
        "the worktree is removed"
    );
    assert_eq!(w.state_of("demo", "a"), MemberState::Failed);
    let agents = w.herdr.run(&["agent", "list"]).unwrap();
    assert!(
        !agents.stdout.contains("tw_demo_a"),
        "the pane is closed: {}",
        agents.stdout
    );
    assert_eq!(
        w.processes_mentioning(w.home.path()),
        Vec::<String>::new(),
        "nothing is left running"
    );
}
