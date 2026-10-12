//! Two teams launched from one lead: a member of one sends a finding to a
//! member of the other as `inv@ops`, and the line lands in that member's pane
//! marked as coming from the other team (ADR 0016).

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_member_of_one_team_reaches_a_member_of_another() {
    let step = format!(
        "if [ \"$TEAMWIRE_TEAM\" = demo ]; then\n\
         teamwire teams | grep -c 'ops (open)' | sed 's/^/teams-listed=/'\n\
         teamwire send --to inv@ops --type finding --text 'the port moved to 8443'; echo xsend-exit=$?\n\
         fi\n{}{}",
        sh::report(false),
        sh::done("member finished")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let ops = json!({"team": "ops", "title": "Ops", "children": [
        {"role": "inv", "title": "Investigator", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "watch the ports"}]});
    let launch = w.launch(&ops, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:ops] inv finished (done)",
        Duration::from_secs(30),
    );

    let demo = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Sender", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "tell ops"}]});
    let launch = w.launch(&demo, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));

    let sender = w.member_screen("demo", "a");
    assert!(sender.contains("teams-listed=1"), "{sender}");
    assert!(sender.contains("xsend-exit=0"), "{sender}");
    let ok = teamwire_e2e::wait_until(Duration::from_secs(10), || {
        w.member_screen("ops", "inv")
            .contains("[teamwire:ops msg m001 finding from a@demo]")
    });
    let receiver = w.member_screen("ops", "inv");
    assert!(ok, "{receiver}");
    assert!(receiver.contains("from team demo"), "{receiver}");
    // Both teams keep the message.
    let log = w.teamwire(&["log", "ops", "--json"], None);
    assert!(log.out.contains("a@demo"), "{}", log.all());
    let log = w.teamwire(&["log", "demo", "--json"], None);
    assert!(log.out.contains("inv@ops"), "{}", log.all());
}
