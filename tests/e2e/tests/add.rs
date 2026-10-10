//! A member added with `--add` after the first wave is watched, and
//! `TEAM-COMPLETE` waits for it.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_member_added_later_is_watched_and_the_team_waits_for_it() {
    let step = format!(
        "{}{}{}",
        "while [ ! -f \"$TEAMWIRE_HOME/go-$TEAMWIRE_ROLE\" ]; do sleep 0.1; done\n",
        sh::report(false),
        sh::done("finished")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let member = |role: &str, title: &str| {
        json!({"role": role, "title": title, "kind": "investigator", "mode": "pane",
               "cwd": w.repo(), "task": "look around"})
    };
    let first = json!({"team": "demo", "title": "Demo", "children": [member("a", "Alpha")]});
    let launch = w.launch(&first, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["a"]);
    let watch = w.watch("demo", 1);
    watch.wait_for("WATCHING 1 members", Duration::from_secs(20));

    let more = json!({"team": "demo", "title": "Demo", "children": [member("b", "Beta")]});
    let add = w.launch(&more, &["--add"]);
    assert_eq!(add.code, 0, "{}", add.all());
    w.wait_started("demo", &["a", "b"]);

    w.write(&w.home.path().join("go-a"), "");
    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));
    // Several polls pass with a finished and b still working. The watch
    // prints nothing per poll, so time is the only signal: three polls at
    // the 1 s interval.
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !watch.output().contains("TEAM-COMPLETE"),
        "the team waits for the added member: {}",
        watch.output()
    );

    w.write(&w.home.path().join("go-b"), "");
    watch.wait_for("TEAM-COMPLETE 2/2 done", Duration::from_secs(60));
    let screen = w.lead_screen();
    assert!(
        screen.contains("[teamwire:demo] b finished (done)"),
        "{screen}"
    );
}
