//! The watch property of `docs/test-strategy.md`: whatever the members, the
//! host and the clock do, a poll prints at most one line, every announcement
//! key is printed exactly once, and `TEAM-COMPLETE` ends a run exactly once
//! when every live member is settled. No line says both `WATCHING` and
//! `TEAM-COMPLETE`, so a watch always polls at least twice (WAT-20).

#[path = "support/fixture.rs"]
pub mod fixture;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use fixture::{Bed, names};
use proptest::prelude::*;
use teamwire_core::{CompletionStatus, DeliveryErrorCode, ReportState};

const MEMBERS: [&str; 4] = ["a", "b", "c", "d"];
/// Present from the start; `d` is added mid-run by `Step::Add`.
const FIRST: [&str; 3] = ["a", "b", "c"];

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
    Add,
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
        1 => Just(Step::Add),
        1 => Just(Step::Restart),
    ]
}

/// What the run has printed, checked line by line.
#[derive(Default)]
struct Run {
    lines: Vec<String>,
    generation: HashMap<&'static str, u32>,
    seen_once: HashMap<String, u32>,
    /// `TEAM-COMPLETE` was printed by the current process.
    complete: bool,
}

impl Run {
    fn record(&mut self, line: &str) {
        assert!(!line.contains('\n'), "{line:?}");
        for event in line.split(" | ") {
            assert!(event.starts_with("[teamwire:alpha] "), "{event}");
            let mut words = event.trim_start_matches("[teamwire:alpha] ").split(' ');
            let (kind, role) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
            if matches!(kind, "DIED" | "LATE") {
                let generation = self.generation.get(role).copied().unwrap_or(0);
                let key = format!("{kind}:{role}:{generation}");
                let times = self.seen_once.entry(key.clone()).or_insert(0);
                *times += 1;
                assert!(
                    *times <= 1,
                    "{key} announced twice: {:?} then {line}",
                    self.lines
                );
            }
        }
        let events = names(line);
        let says = |name: &str| events.iter().any(|e| e == name);
        // WAT-20: the poll that says `WATCHING` leaves `TEAM-COMPLETE` to
        // the next poll, so a member written by a `launch --add` a moment
        // after that poll read the roster is still watched.
        assert!(
            !(says("WATCHING") && says("TEAM-COMPLETE")),
            "the poll that said WATCHING also completed the team: {line}"
        );
        if says("TEAM-COMPLETE") {
            assert!(!self.complete, "TEAM-COMPLETE twice");
            self.complete = true;
        } else {
            assert!(!self.complete, "a line after TEAM-COMPLETE: {line}");
        }
        self.lines.push(line.to_owned());
    }
}

/// Every announcement key was printed exactly once: the printed events of
/// each kind are as many as the stored keys of that kind.
fn assert_each_key_printed_once(bed: &Bed, lines: &[String]) {
    let mut printed: BTreeMap<String, usize> = BTreeMap::new();
    for line in lines {
        for name in names(line) {
            *printed.entry(name).or_insert(0) += 1;
        }
    }
    let stored = bed
        .store
        .read(|tx| tx.announcements()?.all_or_corrupt())
        .unwrap_or_default();
    let mut keys: BTreeMap<String, usize> = BTreeMap::new();
    for a in stored {
        let kind = a.key.split(':').next().unwrap_or("").to_owned();
        *keys.entry(kind).or_insert(0) += 1;
    }
    for (kind, stored) in &keys {
        let shown = printed.get(kind).copied().unwrap_or(0);
        assert_eq!(
            shown, *stored,
            "{kind}: printed {shown}, stored keys {stored}: {lines:?}"
        );
    }
}

fn run(steps: &[Step]) {
    let mut bed = Bed::new();
    let mut present: HashSet<&'static str> = HashSet::new();
    for name in FIRST {
        bed.add_with_deadline(name, 300);
        present.insert(name);
    }
    let mut removed: HashSet<&'static str> = HashSet::new();
    let mut state = Run {
        generation: MEMBERS.iter().map(|m| (*m, 1)).collect(),
        ..Run::default()
    };
    let live = |name: &&'static str, present: &HashSet<&str>, removed: &HashSet<&str>| {
        present.contains(name) && !removed.contains(name)
    };

    for s in steps {
        let mut printed = None;
        match s {
            Step::Tick => printed = bed.tick(),
            Step::Work(i) if live(i, &present, &removed) => bed.working(i),
            Step::Idle(i) if live(i, &present, &removed) => bed.idle(i),
            Step::Block(i) if live(i, &present, &removed) => bed.blocked(i),
            Step::Vanish(i) if live(i, &present, &removed) => bed.vanish(i),
            Step::ListingFails(on) => bed.harness.fail_listing(on.then(|| {
                teamwire_core::HarnessError::new(teamwire_core::HarnessErrorCode::Timeout, "slow")
            })),
            Step::Sleep(secs) => {
                bed.clock.sleep_and_wake(Duration::from_secs(*secs));
                printed = bed.poll();
            }
            Step::Deliver(i, ok) if live(i, &present, &removed) => {
                let status = if *ok {
                    CompletionStatus::Done
                } else {
                    CompletionStatus::Failed
                };
                bed.delivered(i, status);
            }
            Step::Undeliver(i) if live(i, &present, &removed) => {
                let id = bed.complete(i, CompletionStatus::Done);
                bed.finish(
                    &id,
                    ReportState::Undelivered,
                    Some(DeliveryErrorCode::NoHost),
                );
            }
            Step::Reprompt(i) if live(i, &present, &removed) => {
                bed.reprompt(i);
                *state.generation.entry(i).or_insert(1) += 1;
            }
            Step::Remove(i) if live(i, &present, &removed) => {
                bed.remove(i);
                removed.insert(i);
            }
            Step::Add if !present.contains("d") => {
                bed.add_with_deadline("d", 300);
                present.insert("d");
            }
            Step::Restart => {
                // A new process is a new run: it may end with its own
                // `TEAM-COMPLETE`, but repeats nothing else.
                bed.memory = teamwire_watch::WatchMemory::default();
                state.complete = false;
            }
            // A step for a member that is not there, or is gone, does nothing.
            Step::Work(_)
            | Step::Idle(_)
            | Step::Block(_)
            | Step::Vanish(_)
            | Step::Deliver(..)
            | Step::Undeliver(_)
            | Step::Reprompt(_)
            | Step::Remove(_)
            | Step::Add => {}
        }
        if let Some(line) = printed {
            state.record(&line);
        }
    }

    // Every live member finishes and its result reaches the lead: the watch
    // must then say `TEAM-COMPLETE`, once, and be over.
    let live_members: Vec<&'static str> = MEMBERS
        .iter()
        .copied()
        .filter(|m| live(m, &present, &removed))
        .collect();
    for name in &live_members {
        bed.delivered(name, CompletionStatus::Done);
    }
    for _ in 0..4 {
        if let Some(line) = bed.tick() {
            state.record(&line);
        }
    }
    if !live_members.is_empty() {
        assert!(
            state.complete,
            "every live member is settled but the watch never completed: {:?}",
            state.lines
        );
        assert!(bed.memory.end.is_some());
    }
    assert_each_key_printed_once(&bed, &state.lines);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn watch_lines_obey_the_invariants(steps in proptest::collection::vec(step(), 1..80)) {
        run(&steps);
    }
}
