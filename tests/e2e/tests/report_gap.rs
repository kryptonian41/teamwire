//! A member's report lacks a progress line: the Stop hook holds it once
//! with the facts, and the result is delivered on the next stop.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_report_without_a_progress_line_is_held_once_then_delivered() {
    let work = format!(
        "{}{}{}",
        sh::commit(),
        sh::report(false),
        sh::done("built it")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(work)]],
        on_hold: vec![Step::Run(sh::add_progress_line().to_owned())],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());

    w.wait_lead_screen("[teamwire:demo] a finished (done)", Duration::from_secs(30));
    let sha = w.git(
        &w.repo().join(".worktrees/a"),
        &["rev-parse", "--short=7", "HEAD"],
    );
    let member = w.member_screen("demo", "a");
    assert_eq!(
        member.matches("Stop hook feedback:").count(),
        1,
        "held exactly once: {member}"
    );
    let report = format!("{}/teams/demo/reports/a.md", w.home.path().display());
    assert!(
        member.contains(&format!(
            "{report} has no progress line for commit(s) {sha}"
        )),
        "the hold states the facts: {member}"
    );
    let lead = w.lead_screen();
    assert_eq!(lead.matches("a finished (done)").count(), 1, "{lead}");
    let verify = w.teamwire(&["verify", "demo"], None);
    assert_eq!(
        verify.code,
        0,
        "the report is complete now: {}",
        verify.all()
    );
}
