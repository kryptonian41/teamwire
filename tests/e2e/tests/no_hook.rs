//! A member whose Stop hook never runs (`fake-claude` in "no hook" mode),
//! with a lead the host cannot type to: the watch prints `COMPLETED` once.
//! The real Claude adapter always reports a turn-end hook, so the route the
//! watch takes here is chosen by the lead's host, not by the harness.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn without_a_turn_end_hook_the_watch_prints_completed_once() {
    let step = format!(
        "{}{}",
        sh::report(false),
        sh::done("finished without a hook")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        no_hook: true,
        ..ClaudeScript::default()
    });
    // The lead is outside herdr, so results reach it through the watch
    // (route B, ADR 0007); the members still run in herdr panes.
    let spec = json!({"team": "demo", "title": "Demo", "host": "herdr", "children": [
        {"role": "inv", "title": "Investigator", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look around"}]});
    let launch = w.launch_outside_herdr(&spec);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["inv"]);
    let watch = w.watch_outside_herdr("demo", 2);
    watch.wait_for("TEAM-COMPLETE 1/1 done", Duration::from_secs(40));

    let output = watch.output();
    assert_eq!(
        output.matches("COMPLETED inv (done)").count(),
        1,
        "{output}"
    );
    let result = format!("{}/teams/demo/outbox/inv.md", w.home.path().display());
    assert!(output.contains(&format!("Result: {result}")), "{output}");
    assert!(std::path::Path::new(&result).is_file());
    assert!(
        !w.lead_screen().contains("[teamwire"),
        "no hook typed anything into the lead: {}",
        w.lead_screen()
    );
}
