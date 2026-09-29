//! A member started by a host that clears the environment still runs
//! `teamwire send` and `teamwire done`, and its Stop hook delivers
//! (`docs/failure-modes.md`, "A member started without its identity
//! variables").

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn member_under_env_clearing_host_can_send_done_and_report() {
    let step = format!(
        "echo identity-vars=$(env | grep -c '^TEAMWIRE_')\n\
         teamwire send --to lead --type finding --text 'seen from the member'; echo send-exit=$?\n\
         {}{}",
        sh::report(false),
        sh::done("member finished")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "inv", "title": "Investigator", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look around"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());

    w.wait_lead_screen(
        "[teamwire:demo] inv finished (done)",
        Duration::from_secs(30),
    );
    let member = w.member_screen("demo", "inv");
    // The host cleared the environment; `settings.json` `env` brought all
    // four variables (team, role, home, token) back.
    assert!(member.contains("identity-vars=4"), "{member}");
    assert!(member.contains("send-exit=0"), "{member}");
    assert!(member.contains("done-exit=0"), "{member}");
    let lead = w.lead_screen();
    assert!(lead.contains("seen from the member"), "{lead}");
    assert_eq!(lead.matches("inv finished (done)").count(), 1, "{lead}");
    for stream in [&launch.out, &launch.err, &member, &lead] {
        assert!(
            !stream.contains("TEAMWIRE_TOKEN="),
            "the token is never printed: {stream}"
        );
    }
}
