//! ADR 0059, through the real binary and the member's real Stop hook: a
//! reprompt over 300 characters whose marker sits only past the cut. The
//! member's turn runs a tool, whose hook reminds it of the reprompt, then
//! answers in chat and records nothing. The Stop hook holds it
//! once; the held turn reads the inbox, writes the marker into its report
//! and records `done`. The watch then counts one result for the reprompt's
//! generation, and the team completes once.

use std::time::Duration;

use serde_json::json;
use teamwire_core::{EventType, Role};
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

const MARKER: &str = "violet-73";

/// The second turn runs one tool, whose `PostToolUse` hook reminds the
/// member of the unread reprompt, then answers in chat and records nothing:
/// the order of the failure that was seen. The hook gets the input Claude
/// Code sends, with the session id the test wrote for it.
const TOOL_THEN_CHAT: &str = "sid=$(cat \"$TEAMWIRE_HOME/a-session\")\n\
     printf '{\"session_id\":\"%s\",\"hook_event_name\":\"PostToolUse\",\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"ls\"},\"cwd\":\"%s\"}' \"$sid\" \"$PWD\" \\\n\
     | teamwire hook claude-post-tool --team \"$TEAMWIRE_TEAM\" --role \"$TEAMWIRE_ROLE\" \\\n\
     > \"$TEAMWIRE_HOME/tool-hook-out\"\n\
     echo 'the answer is 42'\n";

#[test]
fn a_clipped_reprompt_answered_in_chat_is_held_once_then_finished_from_the_inbox() {
    // Turn 1 finishes at once. Every later turn answers in chat: it runs
    // nothing and records no result. A hold runs `on_hold`, which reads the
    // full reprompt and finishes with the marker it found there.
    let finish_from_inbox = String::from(
        "marker=$(teamwire inbox | grep -o 'violet-[0-9]*' | head -n 1)\n\
         echo \"$marker\" > \"$TEAMWIRE_HOME/marker-seen\"\n\
         mkdir -p \"$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports\"\n\
         printf '# report\\n\\nWrote %s.\\n\\nDecisions I made: none\\n' \"$marker\" \\\n\
         > \"$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports/$TEAMWIRE_ROLE.md\"\n\
         teamwire done --status done --summary \"part two $marker\"; echo done-exit=$?\n",
    );
    let w = World::new(&ClaudeScript {
        turns: vec![
            vec![Step::Run(format!(
                "{}{}",
                sh::report(false),
                sh::done("part one")
            ))],
            vec![Step::Run(TOOL_THEN_CHAT.to_owned())],
        ],
        on_hold: vec![Step::Run(finish_from_inbox)],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look at a"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));

    let a = Role::parse("a").unwrap();
    let session = w
        .store("demo")
        .read(|tx| tx.member(&a))
        .unwrap()
        .and_then(|m| m.session_id)
        .unwrap();
    assert!(w.sh_passes(
        w.home.path(),
        &format!("printf %s '{}' > a-session", session.as_str())
    ));
    let text = format!(
        "{} then write {MARKER} in your report",
        "context ".repeat(40)
    );
    let reprompt = w.teamwire(
        &[
            "send", "demo", "--from", "lead", "--to", "a", "--type", "reprompt", "--text", &text,
        ],
        None,
    );
    assert_eq!(reprompt.code, 0, "{}", reprompt.all());
    let watch = w.watch("demo", 1);
    watch.wait_for("TEAM-COMPLETE 1/1 done", Duration::from_secs(60));

    // The typed line held a pointer, not the task.
    let screen = w.member_screen("demo", "a");
    assert!(screen.contains("too long for this line"), "{screen}");
    assert!(!screen.contains("context context"), "{screen}");

    // The tool hook reminded the reprompt before the stop.
    let reminded = std::fs::read_to_string(w.home.path().join("tool-hook-out")).unwrap();
    assert!(reminded.contains("m001"), "{reminded}");
    let store = w.store("demo");
    let held: Vec<_> = store
        .read(|tx| tx.events_of_type(EventType::MemberHeld))
        .unwrap()
        .items
        .into_iter()
        .filter_map(|e| e.data.get("corrective").cloned())
        .collect();
    assert_eq!(held.len(), 1, "{held:?}");
    let done = store.read(|tx| tx.completions_of(&a)).unwrap().items;
    assert_eq!(done.len(), 2, "one result per generation: {done:?}");
    let second = done.last().unwrap();
    assert_eq!(second.completion.generation, 2);
    assert!(
        second.completion.summary.contains(MARKER),
        "{:?}",
        second.completion
    );
    let report = std::fs::read_to_string(w.home.path().join("teams/demo/reports/a.md")).unwrap();
    assert!(report.contains(MARKER), "{report}");
    let lines = w.lead_screen().matches("a finished (done)").count();
    assert_eq!(lines, 2, "{}", w.lead_screen());
    assert_eq!(watch.output().matches("TEAM-COMPLETE").count(), 1);
}
