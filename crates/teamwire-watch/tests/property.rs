//! The watch property of `docs/test-strategy.md`: whatever the members, the
//! host and the clock do, a poll prints at most one line, an occurrence is
//! announced once, and `TEAM-COMPLETE` ends the run exactly once.

#[path = "support/fixture.rs"]
pub mod fixture;

use std::collections::HashMap;
use std::time::Duration;

use fixture::{Bed, names};
use proptest::prelude::*;
use teamwire_core::{CompletionStatus, DeliveryErrorCode, ReportState};

const MEMBERS: [&str; 3] = ["a", "b", "c"];

#[derive(Debug, Clone)]
enum Step {
    Tick,
    Work(&'static str),
    Idle(&'static str),
    Block(&'static str),
    Vanish(&'static str),
    ListingFails(bool),
    Sleep(u64),
    Deliver(&'static str, bool),
    Undeliver(&'static str),
    Reprompt(&'static str),
    Remove(&'static str),
    Restart,
}

fn step() -> impl Strategy<Value = Step> {
    let member = proptest::sample::select(MEMBERS.to_vec());
    prop_oneof![
        6 => Just(Step::Tick),
        2 => member.clone().prop_map(Step::Work),
        2 => member.clone().prop_map(Step::Idle),
        1 => member.clone().prop_map(Step::Block),
        1 => member.clone().prop_map(Step::Vanish),
        1 => any::<bool>().prop_map(Step::ListingFails),
        1 => (1_u64..7200).prop_map(Step::Sleep),
        2 => (member.clone(), any::<bool>()).prop_map(|(m, ok)| Step::Deliver(m, ok)),
        1 => member.clone().prop_map(Step::Undeliver),
        1 => member.clone().prop_map(Step::Reprompt),
        1 => member.prop_map(Step::Remove),
        1 => Just(Step::Restart),
    ]
}

fn run(steps: &[Step]) {
    let mut bed = Bed::new();
    for name in MEMBERS {
        bed.add_with_deadline(name, 300);
    }
    let mut removed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut generation: HashMap<&str, u32> = MEMBERS.iter().map(|m| (*m, 1)).collect();
    let mut lines: Vec<String> = Vec::new();
    let mut seen_once: HashMap<String, u32> = HashMap::new();
    let mut complete_at: Option<usize> = None;

    for s in steps {
        let mut lit = None;
        match s {
            Step::Tick => lit = bed.tick(),
            Step::Work(i) => bed.working(i),
            Step::Idle(i) => bed.idle(i),
            Step::Block(i) => bed.blocked(i),
            Step::Vanish(i) => bed.vanish(i),
            Step::ListingFails(on) => bed.harness.fail_listing(on.then(|| {
                teamwire_core::HarnessError::new(teamwire_core::HarnessErrorCode::Timeout, "slow")
            })),
            Step::Sleep(secs) => {
                bed.clock.sleep_and_wake(Duration::from_secs(*secs));
                lit = bed.poll();
            }
            Step::Deliver(i, ok) if !removed.contains(i) => {
                let status = if *ok {
                    CompletionStatus::Done
                } else {
                    CompletionStatus::Failed
                };
                bed.delivered(i, status);
            }
            Step::Undeliver(i) if !removed.contains(i) => {
                let id = bed.complete(i, CompletionStatus::Done);
                bed.finish(
                    &id,
                    ReportState::Undelivered,
                    Some(DeliveryErrorCode::NoHost),
                );
            }
            Step::Reprompt(i) if !removed.contains(i) => {
                bed.reprompt(i);
                *generation.entry(*i).or_insert(1) += 1;
            }
            Step::Remove(i) if !removed.contains(i) => {
                bed.remove(i);
                removed.insert(*i);
            }
            Step::Restart => {
                // A new process is a new run: it may end with its own
                // `TEAM-COMPLETE`, but repeats nothing else.
                bed.memory = teamwire_watch::WatchMemory::default();
                complete_at = None;
            }
            Step::Deliver(..) | Step::Undeliver(_) | Step::Reprompt(_) | Step::Remove(_) => {}
        }
        let Some(line) = lit else { continue };
        prop_assert_no_newline(&line);
        for event in line.split(" | ") {
            assert!(event.starts_with("[teamwire:alpha] "), "{event}");
        }
        // One occurrence, one line: a death and a deadline are announced
        // once per member and generation, whatever else happens.
        for event in line.split(" | ") {
            let mut words = event.trim_start_matches("[teamwire:alpha] ").split(' ');
            let (kind, role) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
            if matches!(kind, "DIED" | "LATE") {
                let gen_now = generation.get(role).copied().unwrap_or(0);
                let key = format!("{kind}:{role}:{gen_now}");
                let times = seen_once.entry(key.clone()).or_insert(0);
                *times += 1;
                assert!(*times <= 1, "{key} announced twice: {lines:?} then {line}");
            }
        }
        if names(&line).iter().any(|n| n == "TEAM-COMPLETE") {
            assert!(complete_at.is_none(), "TEAM-COMPLETE twice");
            complete_at = Some(lines.len());
        } else {
            assert!(complete_at.is_none(), "a line after TEAM-COMPLETE: {line}");
        }
        lines.push(line);
    }
}

fn prop_assert_no_newline(line: &str) {
    assert!(!line.contains('\n'), "{line:?}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn watch_lines_obey_the_invariants(steps in proptest::collection::vec(step(), 1..80)) {
        run(&steps);
    }
}
