//! The lead's pane is taken over by another session before a member
//! finishes: nothing is typed into it, the watch says `UNDELIVERED` once,
//! and `status` shows the result path.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_result_is_not_typed_into_a_pane_another_session_took_over() {
    let work = format!(
        "{}{}{}{}",
        sh::wait_for("go"),
        sh::commit(),
        sh::report(true),
        sh::done("finished late")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(work)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["a"]);
    let watch = w.watch("demo", 2);
    watch.wait_for("WATCHING 1 members", Duration::from_secs(20));

    w.take_over_lead_pane("intruder-1");
    let before = w.screen_of("lead2");
    w.write(&w.home.path().join("go"), "");
    watch.wait_for(
        "TEAM-COMPLETE 0/1 done; not done: a undelivered",
        Duration::from_secs(60),
    );

    let output = watch.output();
    assert_eq!(output.matches("UNDELIVERED a").count(), 1, "{output}");
    let result = format!("{}/teams/demo/outbox/a.md", w.home.path().display());
    assert!(
        output.contains(&format!("Result: {result}")),
        "the watch names the result: {output}"
    );
    let after = w.screen_of("lead2");
    assert_eq!(before, after, "nothing was typed into the other session");
    assert!(!after.contains("[teamwire"), "{after}");

    let status = w.teamwire(&["status", "demo"], None);
    assert!(status.out.contains(&result), "{}", status.all());
    assert!(std::path::Path::new(&result).is_file());
}
