//! `kill -9` during `teamwire done`: a completion is stored whole or not at
//! all, and the next `done` numbers on from what is there
//! (`docs/test-strategy.md` section 3).
#![expect(
    clippy::disallowed_methods,
    reason = "the test measures real elapsed time to place its kill points"
)]
#![expect(
    clippy::print_stderr,
    reason = "the kill window and the counts are printed so a failing run can be read"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::time::{Duration, Instant};

use rig::{Identity, Rig, assert_integrity};
use teamwire_core::{MemberKind, Role};

const KILL_POINTS: usize = 60;

fn done_args(summary: &str) -> Vec<&str> {
    vec!["done", "--status", "done", "--summary", summary]
}

fn numbers(rig: &Rig) -> Vec<u64> {
    rig.store()
        .read(|tx| tx.completions_of(&Role::parse("api").unwrap()))
        .unwrap()
        .items
        .iter()
        .map(|c| u64::from(c.completion.n))
        .collect()
}

#[test]
fn kill9_during_done_stores_whole_or_nothing() {
    let mut rng = rig::Rng::from_env("kill9_during_done_stores_whole_or_nothing");
    let rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer, false);
    let me = Identity::Member("api".to_owned());

    let mut times: Vec<Duration> = (0..3)
        .map(|_| {
            let started = Instant::now();
            let out = rig.run(&done_args("warm-up"), &me);
            assert_eq!(out.code, Some(0), "{out:?}");
            started.elapsed()
        })
        .collect();
    times.sort();
    let window = times[1] * 13 / 10;
    eprintln!("kill window {window:?}");

    let mut told = 0;
    for i in 0..KILL_POINTS {
        let summary = format!("kill-{i}");
        let before = numbers(&rig);
        let delay = rng.up_to(if i % 2 == 0 { window } else { window / 5 });
        let out = rig.run_and_kill(&done_args(&summary), &me, delay);
        assert!(!out.stderr.contains("refused"), "{out:?}");

        let store = rig.store();
        assert_integrity(&store);
        let after = numbers(&rig);
        // The completion is stored whole or not at all, and the numbers
        // stay one run of consecutive integers.
        assert!(
            after.len() == before.len() || after.len() == before.len() + 1,
            "{before:?} -> {after:?} after a kill at {delay:?}"
        );
        assert!(
            after.iter().copied().eq(1..=after.len() as u64),
            "completion numbers {after:?}"
        );
        // A child that told the member it recorded a completion did record it.
        if out.stdout.contains("recorded completion") {
            told += 1;
            assert_eq!(after.len(), before.len() + 1, "{out:?}");
        }
    }
    eprintln!("{told} of {KILL_POINTS} runs printed their completion before the kill");

    // After the last kill a plain `done` is stored as the next number.
    let count = numbers(&rig).len();
    let out = rig.run(&done_args("last"), &me);
    assert_eq!(out.code, Some(0), "{out:?}");
    assert!(
        out.stdout
            .contains(&format!("recorded completion {}", count + 1)),
        "{out:?}"
    );
    assert_eq!(numbers(&rig).len(), count + 1);
    assert_integrity(&rig.store());
}
