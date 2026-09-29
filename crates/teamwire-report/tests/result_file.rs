//! The result file: who wrote it, where the final message came from, and what
//! happens to a file the member wrote at that path.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    reason = "a failing unwrap or index in an integration test fails the test, and the test stands in for a member writing its own files"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/kit.rs"]
mod kit;
#[path = "../../teamwire-deliver/tests/support/members.rs"]
mod members;
#[path = "../../teamwire-deliver/tests/support/rig.rs"]
mod rig;

use teamwire_core::{FinalSource, TurnEnd, TurnEndReply};
use teamwire_report::on_turn_end;

use kit::{stop, team};

const REPORT: &str = "Decisions I made: none\n";

fn finish(rig: &mut rig::Rig, api: &teamwire_core::Role, t: &TurnEnd) {
    rig.write_report(api, REPORT);
    rig.complete_now(api);
    assert_eq!(on_turn_end(&mut rig.ctx(), api, t), TurnEndReply::Continue);
}

fn outbox(rig: &rig::Rig) -> String {
    std::fs::read_to_string(rig.team_dir().join("outbox/api.md")).unwrap()
}

/// A transcript whose last assistant line is a mid-work line.
fn transcript(rig: &rig::Rig, last: &str) -> std::path::PathBuf {
    let path = rig.home.path().join("transcript.jsonl");
    let lines = [
        r#"{"type":"user","message":{"content":"do it"}}"#.to_owned(),
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Early line"}]}}"#
            .to_owned(),
        "not json at all".to_owned(),
        format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"x"}},{{"type":"text","text":"{last}"}}]}}}}"#
        ),
    ];
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

#[test]
fn hook_message_wins_over_transcript() {
    let (mut rig, api) = team();
    let mut t = stop("api-1");
    t.final_message = Some("The real final.".to_owned());
    t.transcript = Some(transcript(
        &rig,
        "Running the full verification command now.",
    ));
    finish(&mut rig, &api, &t);
    let text = outbox(&rig);
    assert!(text.contains("The real final."), "{text}");
    assert!(!text.contains("Running the full verification"), "{text}");
    assert!(text.contains("Final message from: hook"), "{text}");
    assert_eq!(rig.newest(&api).completion.final_source, FinalSource::Hook);
}

#[test]
fn transcript_is_the_fallback_and_the_file_says_so() {
    let (mut rig, api) = team();
    let mut t = stop("api-1");
    t.final_message = None;
    t.transcript = Some(transcript(&rig, "From the transcript."));
    finish(&mut rig, &api, &t);
    let text = outbox(&rig);
    assert!(text.contains("From the transcript."), "{text}");
    assert!(!text.contains("Early line"), "only the last assistant text");
    assert!(text.contains("Final message from: transcript"), "{text}");
    assert_eq!(
        rig.newest(&api).completion.final_source,
        FinalSource::Transcript
    );
}

#[test]
fn no_final_message_at_all_is_said_plainly() {
    let (mut rig, api) = team();
    let mut t = stop("api-1");
    t.final_message = Some("   ".to_owned());
    t.transcript = Some(rig.home.path().join("missing.jsonl"));
    finish(&mut rig, &api, &t);
    let text = outbox(&rig);
    assert!(text.contains("Final message from: none"), "{text}");
    assert!(text.contains("not available"), "{text}");
    assert_eq!(rig.newest(&api).completion.final_source, FinalSource::None);
}

#[test]
fn unmarked_file_rescued_and_linked() {
    let (mut rig, api) = team();
    let outbox_dir = rig.team_dir().join("outbox");
    std::fs::create_dir_all(&outbox_dir).unwrap();
    std::fs::write(outbox_dir.join("api.md"), "My own long report.\n").unwrap();
    finish(&mut rig, &api, &stop("api-1"));

    let text = outbox(&rig);
    assert!(
        text.starts_with("<!-- teamwire:generated completion="),
        "{text}"
    );
    let rescued: Vec<_> = std::fs::read_dir(rig.team_dir().join("reports"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains("api.rescued-"))
        .collect();
    assert_eq!(rescued.len(), 1, "{rescued:?}");
    assert_eq!(
        std::fs::read_to_string(&rescued[0]).unwrap(),
        "My own long report.\n"
    );
    assert!(
        text.contains(&rescued[0].display().to_string()),
        "linked: {text}"
    );
}

#[test]
fn a_generated_file_is_replaced_not_rescued() {
    let (mut rig, api) = team();
    finish(&mut rig, &api, &stop("api-1"));
    rig.store
        .write(|tx| tx.bump_generation(kit::at(30), &api, "again").map(|_| ()))
        .unwrap();
    rig.complete_now(&api);
    let mut t = stop("api-1");
    t.final_message = Some("Second result.".to_owned());
    on_turn_end(&mut rig.ctx(), &api, &t);
    assert!(outbox(&rig).contains("Second result."));
    let rescued = std::fs::read_dir(rig.team_dir().join("reports"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .to_string_lossy()
                .contains("rescued")
        })
        .count();
    assert_eq!(rescued, 0);
}

#[test]
fn the_file_shows_the_facts_the_lead_needs() {
    let (mut rig, api) = team();
    let briefs = rig.team_dir().join("briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    std::fs::write(
        briefs.join("api.md"),
        format!("Build the api.\n{}", "x".repeat(500)),
    )
    .unwrap();
    finish(&mut rig, &api, &stop("api-1"));
    let text = outbox(&rig);
    for needle in [
        "Status: done",
        "Generation: 1",
        "Completion: 1",
        "finished the api",
        "reports/api.md",
        "Build the api.",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    assert!(
        !text.contains(&"x".repeat(400)),
        "the task is cut to 300 characters"
    );
}
