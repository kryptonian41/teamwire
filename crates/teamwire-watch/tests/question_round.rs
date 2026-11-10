//! A member that stops with questions for the lead (a question round) waits
//! for the lead's reprompt. The watch keeps polling and follows the
//! member's next generation, rather than calling the team complete.
#[path = "support/fixture.rs"]
pub mod fixture;

use std::time::Duration;

use fixture::{Bed, count, counts};
use teamwire_core::{CompletionStatus, ReportState};
use teamwire_watch::WatchEnd;

const NO_CLOSER: &str = ". If no wave is due: add the integrator when branches join, then the closer (teamwire preset closer).";

/// A member past its deadline that has stopped with `summary`, its result
/// delivered to the lead.
fn stopped(bed: &mut Bed, name: &str, summary: &str) {
    bed.add_with_deadline(name, 100);
    bed.age(Duration::from_secs(120));
    let id = bed.complete_with(name, CompletionStatus::Blocked, summary);
    bed.finish(&id, ReportState::Delivered, None);
}

#[test]
fn a_question_round_keeps_the_watch_and_follows_the_reprompt() {
    let mut bed = Bed::new();
    stopped(
        &mut bed,
        "spec",
        "2 questions for the parent. Scope and auth.",
    );
    let mut lines: Vec<String> = bed.poll().into_iter().collect();
    lines.extend(bed.ticks(30));
    let got = counts(&lines);
    for name in ["TEAM-COMPLETE", "SILENT", "LATE", "TEAM-STALLED"] {
        assert_eq!(got.get(name), None, "{name}: {lines:?}");
    }
    assert_eq!(bed.memory.end, None);

    // The lead answers with a reprompt: the same watch follows the new run.
    bed.reprompt("spec");
    bed.working("spec");
    assert!(bed.ticks(3).is_empty());
    bed.delivered("spec", CompletionStatus::Done);
    let end = bed.tick().unwrap();
    assert!(
        end.ends_with(&format!("TEAM-COMPLETE 1/1 done{NO_CLOSER}")),
        "{end}"
    );
    assert_eq!(bed.memory.end, Some(WatchEnd::TeamComplete));
}

#[test]
fn one_question_and_a_handoff_first_are_question_rounds() {
    for summary in [
        "1 question for the parent. Which branch?",
        "handoff: 12 questions for the parent. See the report.",
    ] {
        let mut bed = Bed::new();
        stopped(&mut bed, "spec", summary);
        bed.poll();
        let lines = bed.ticks(10);
        assert_eq!(count(&lines, "TEAM-COMPLETE"), 0, "{summary}: {lines:?}");
    }
}

#[test]
fn a_blocked_result_in_any_other_words_settles_the_member() {
    for summary in [
        "0 questions for the parent.",
        "3 questions for the lead.",
        "3 Questions for the parent.",
        "3  questions for the parent.",
        "3 questions for the parent",
        "Blocked: 3 questions for the parent.",
        "handoff:3 questions for the parent.",
    ] {
        let mut bed = Bed::new();
        stopped(&mut bed, "spec", summary);
        bed.poll();
        let lines = bed.ticks(2);
        assert_eq!(
            lines
                .last()
                .map(String::as_str)
                .map(|l| l.ends_with(&format!(
                    "TEAM-COMPLETE 0/1 done; not done: spec blocked{NO_CLOSER}"
                ))),
            Some(true),
            "{summary}: {lines:?}"
        );
    }
}

#[test]
fn only_a_blocked_status_is_a_question_round() {
    let mut bed = Bed::new();
    bed.add("spec");
    bed.poll();
    let id = bed.complete_with(
        "spec",
        CompletionStatus::Failed,
        "2 questions for the parent.",
    );
    bed.finish(&id, ReportState::Delivered, None);
    let line = bed.tick().unwrap();
    assert!(
        line.contains("TEAM-COMPLETE 0/1 done; not done: spec failed"),
        "{line}"
    );
}

/// The member waiting for answers is not stuck, so the rest of the team is
/// judged without it: every other member stuck still stalls the team.
#[test]
fn others_stuck_beside_a_question_round_still_stall_the_team() {
    let mut bed = Bed::new();
    stopped(&mut bed, "spec", "2 questions for the parent.");
    bed.add_with_deadline("api", 100);
    bed.age(Duration::from_secs(120));
    let mut lines: Vec<String> = bed.poll().into_iter().collect();
    lines.extend(bed.ticks(30));
    let stalls: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("TEAM-STALLED"))
        .collect();
    assert_eq!(stalls.len(), 1, "{lines:?}");
    assert!(
        stalls.iter().all(|l| l.contains("TEAM-STALLED api")),
        "{stalls:?}"
    );
}

/// A question round whose result never reached the lead is announced, and
/// the member still waits for the lead.
#[test]
fn an_undelivered_question_round_is_announced_and_still_waits() {
    let mut bed = Bed::new();
    bed.add("spec");
    bed.poll();
    let id = bed.complete_with(
        "spec",
        CompletionStatus::Blocked,
        "2 questions for the parent.",
    );
    bed.finish(
        &id,
        ReportState::Undelivered,
        Some(teamwire_core::DeliveryErrorCode::DeliveryInterrupted),
    );
    let lines = bed.ticks(5);
    assert_eq!(count(&lines, "UNDELIVERED"), 1, "{lines:?}");
    assert_eq!(count(&lines, "TEAM-COMPLETE"), 0, "{lines:?}");
}
