//! A launch whose setup fails for one member: exit 2, that member refused
//! and cleaned up, the others run.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_failing_setup_fails_one_member_and_the_others_run() {
    let step = format!("{}{}", sh::report(false), sh::done("a finished"));
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]},
        {"role": "b", "title": "Beta", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build b", "check": "true", "owns": ["b.txt"],
         "setup": "false"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 2, "{}", launch.all());
    assert!(launch.out.contains("a: started"), "{}", launch.all());
    assert!(launch.out.contains("b: failed:"), "{}", launch.all());
    assert!(
        launch.out.contains("1/2 members started"),
        "{}",
        launch.all()
    );
    assert!(
        !w.repo().join(".worktrees/b").exists(),
        "b's checkout was taken back"
    );
    assert!(w.repo().join(".worktrees/a").is_dir());

    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));
    let status = w.teamwire(&["status", "demo", "--json"], None);
    assert!(status.out.contains("failed to start"), "{}", status.all());
    // The fake herdr lists the agents it started. A command line would not
    // name b's checkout, so the agent list is what shows that b never ran.
    let agents = w.herdr.run(&["agent", "list"]).unwrap().stdout;
    assert!(agents.contains("tw_demo_a"), "a is running: {agents}");
    assert!(
        !agents.contains("tw_demo_b"),
        "nothing runs for b: {agents}"
    );
}
