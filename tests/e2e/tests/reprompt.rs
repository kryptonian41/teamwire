//! A reprompt to a finished member: the watch keeps running, and the second
//! completion is a second result.

use std::time::Duration;

use serde_json::json;
use teamwire_core::Role;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_reprompt_to_a_finished_member_makes_a_second_result() {
    // `a` finishes at once and again after a reprompt; `b` finishes when the
    // test says, so the watch has something to wait for in between.
    let a = format!("{}{}", sh::report(false), sh::done("a finished"));
    let b = format!(
        "{}{}{}",
        sh::wait_for("go-b"),
        sh::report(false),
        sh::done("b finished")
    );
    let step = format!("if [ \"$TEAMWIRE_ROLE\" = a ]; then\n{a}else\n{b}fi\n");
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look at a"},
        {"role": "b", "title": "Beta", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look at b"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["a", "b"]);
    let watch = w.watch("demo", 2);
    watch.wait_for("WATCHING 2 members", Duration::from_secs(20));
    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));

    let reprompt = w.teamwire(
        &[
            "send",
            "demo",
            "--from",
            "lead",
            "--to",
            "a",
            "--type",
            "reprompt",
            "--text",
            "look again",
        ],
        None,
    );
    assert_eq!(reprompt.code, 0, "{}", reprompt.all());
    assert!(
        reprompt.out.to_lowercase().contains("watch"),
        "the reprompt says a watch is running: {}",
        reprompt.out
    );
    let ok = teamwire_e2e::wait_until(Duration::from_secs(30), || {
        w.lead_screen().matches("a finished (done)").count() == 2
    });
    assert!(ok, "a second result for a: {}", w.lead_screen());

    let store = w.store("demo");
    let done = store
        .read(|tx| tx.completions_of(&Role::parse("a").unwrap()))
        .unwrap()
        .items;
    assert_eq!(done.len(), 2, "two completions, one per generation");
    assert!(
        !watch.output().contains("TEAM-COMPLETE"),
        "the team is not complete while b is working: {}",
        watch.output()
    );

    w.write(&w.home.path().join("go-b"), "");
    watch.wait_for("TEAM-COMPLETE 2/2 done", Duration::from_secs(60));
    assert_eq!(watch.output().matches("TEAM-COMPLETE").count(), 1);
}
