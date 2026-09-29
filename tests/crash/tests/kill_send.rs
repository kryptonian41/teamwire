//! `kill -9` during `teamwire send`, then the store is reopened and checked
//! (`docs/test-strategy.md` section 3, `docs/failure-modes.md` section 2).
#![expect(
    clippy::disallowed_methods,
    reason = "the test measures real elapsed time to place its kill points"
)]
#![expect(
    clippy::print_stderr,
    reason = "the kill window and the counts are printed so a failing run can be read"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file index; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rig::{Identity, Rig, all_messages, assert_integrity, count_with_body, recipient};
use teamwire_core::{DeliveryState, MemberKind};

const KILL_POINTS: usize = 200;
const WORKERS: usize = 9;

/// One sender and one recipient. The lead is the sender when `from` is `None`.
#[derive(Clone)]
struct Pair {
    from: Option<String>,
    to: String,
}

impl Pair {
    fn identity(&self) -> Identity {
        self.from.clone().map_or(Identity::Lead, Identity::Member)
    }
}

/// Every sender to every recipient. A sender may send only five messages to
/// one recipient in a minute, so 200 kill points need many pairs.
fn pairs() -> Vec<Pair> {
    let roles: Vec<String> = (1..=WORKERS).map(|i| format!("w{i}")).collect();
    let mut out = Vec::new();
    for to in &roles {
        out.push(Pair {
            from: None,
            to: to.clone(),
        });
        for from in roles.iter().filter(|r| *r != to) {
            out.push(Pair {
                from: Some(from.clone()),
                to: to.clone(),
            });
        }
    }
    out
}

fn team_of_workers() -> Rig {
    let rig = Rig::new();
    for i in 1..=WORKERS {
        rig.add_member(&format!("w{i}"), MemberKind::Investigator, true);
    }
    rig
}

fn send_args<'a>(pair: &'a Pair, key: &'a str) -> Vec<&'a str> {
    let mut args = vec!["send", "alpha"];
    if pair.from.is_none() {
        args.extend(["--from", "lead"]);
    }
    let kind = if pair.from.is_none() {
        "note"
    } else {
        "finding"
    };
    args.extend([
        "--to", &pair.to, "--type", kind, "--text", key, "--key", key,
    ]);
    args
}

/// The id a child printed on its "stored mNNN (key K)" line, if it got that
/// far before the kill.
fn printed_id(stderr: &str, key: &str) -> Option<String> {
    let marker = format!("(key {key})");
    stderr.lines().find_map(|line| {
        let rest = line.strip_prefix("stored ")?;
        let (id, tail) = rest.split_once(' ')?;
        tail.starts_with(&marker).then(|| id.to_owned())
    })
}

/// How long an unkilled send takes here: the median of a few runs, each on
/// a pair the kill loop uses only three times.
fn typical_send(rig: &Rig, pairs: &[Pair]) -> Duration {
    let mut times: Vec<Duration> = pairs
        .iter()
        .take(5)
        .enumerate()
        .map(|(i, pair)| {
            let key = format!("warmup-{i}");
            let started = Instant::now();
            let out = rig.run(&send_args(pair, &key), &pair.identity());
            assert_eq!(out.code, Some(0), "{out:?}");
            started.elapsed()
        })
        .collect();
    times.sort();
    times[times.len() / 2]
}

#[test]
fn kill9_during_send_keeps_integrity() {
    let mut rng = rig::Rng::from_env("kill9_during_send_keeps_integrity");
    let rig = team_of_workers();
    let pairs = pairs();
    // Kill points spread over a little more than a whole send, so some hit
    // before the row is stored and some after the last host call.
    let window = typical_send(&rig, &pairs) * 13 / 10;
    eprintln!("kill window {window:?}");

    let mut acknowledged: BTreeMap<String, (String, Pair)> = BTreeMap::new();
    let mut stored_at_kill = 0;
    for i in 0..KILL_POINTS {
        let key = format!("crash-{i:03}");
        // Half the kill points crowd into the first fifth, where the row is written.
        let delay = rng.up_to(if i % 2 == 0 { window } else { window / 5 });
        let pair = &pairs[(i + 5) % pairs.len()];
        let out = rig.run_and_kill(&send_args(pair, &key), &pair.identity(), delay);
        assert!(!out.stderr.contains("refused"), "{key}: {out:?}");
        let store = rig.store();
        assert_integrity(&store);
        let rows = count_with_body(&store, &key);
        assert!(
            rows <= 1,
            "{key} stored {rows} times after a kill at {delay:?}"
        );
        if let Some(id) = printed_id(&out.stderr, &key) {
            assert_eq!(
                rows, 1,
                "{key}: {id} was printed as stored, the row is gone"
            );
            acknowledged.insert(key.clone(), (id, pair.clone()));
        }
        stored_at_kill += rows;
    }
    eprintln!(
        "{stored_at_kill} of {KILL_POINTS} sends had a row at the kill; {} printed their id",
        acknowledged.len()
    );

    assert!(
        acknowledged.len() >= KILL_POINTS / 10,
        "only {} sends got as far as printing their id; the kill window is off",
        acknowledged.len()
    );

    // Every acknowledged id is present exactly once, under the id it was told.
    let store = rig.store();
    assert_integrity(&store);
    for (key, (id, _)) in &acknowledged {
        let with_id: Vec<_> = all_messages(&store)
            .into_iter()
            .filter(|m| m.display_id.as_str() == id)
            .collect();
        assert_eq!(with_id.len(), 1, "{key}: id {id}");
        assert_eq!(&with_id[0].body, key);
    }

    // Sending every key again makes no second row and returns the same id.
    let rows_before = all_messages(&store).len();
    for (key, (id, pair)) in &acknowledged {
        let out = rig.run(&send_args(pair, key), &pair.identity());
        assert!(
            out.stdout.starts_with(&format!("{id}\n")),
            "{key}: expected {id}, got {out:?}"
        );
        assert_eq!(count_with_body(&rig.store(), key), 1, "{key}");
    }
    let store = rig.store();
    assert_integrity(&store);
    let after = all_messages(&store);
    assert_eq!(after.len(), rows_before, "resending created a row");
    let mut ids: Vec<_> = after.iter().map(|m| m.display_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), after.len(), "two rows share a display id");
}

#[test]
fn kill_after_submit_at_most_one_duplicate() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer, true);
    let signal = rig.pause_after("agent prompt");

    let key = "after-submit";
    let to_api = Pair {
        from: None,
        to: "api".to_owned(),
    };
    let mut running = rig.start(&send_args(&to_api, key), &Identity::Lead);
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    let out = running.finish();
    let id = printed_id(&out.stderr, key).expect("the id was printed before the host call");

    // The text reached the pane once, and the outcome was never recorded.
    assert_eq!(rig.typed_count(key), 1);
    let store = rig.store();
    assert_integrity(&store);
    assert_eq!(count_with_body(&store, key), 1);
    let message = all_messages(&store).pop().unwrap();
    assert_eq!(message.display_id.as_str(), id);
    let state = |rig: &Rig| {
        rig.store()
            .read(|tx| tx.delivery(&message.ulid, &recipient("api")))
            .unwrap()
            .unwrap()
            .delivery
            .state
    };
    assert_eq!(state(&rig), DeliveryState::Pending);

    // The resume types it again: the one allowed duplicate.
    rig.path_prefix = None;
    let out = rig.run(&send_args(&to_api, key), &Identity::Lead);
    assert!(out.stdout.starts_with(&format!("{id}\n")), "{out:?}");
    assert_eq!(rig.typed_count(key), 2);
    assert_eq!(state(&rig), DeliveryState::HandedOff);

    // Nothing more is typed, and the member's inbox holds it once.
    let out = rig.run(&send_args(&to_api, key), &Identity::Lead);
    assert!(out.stdout.starts_with(&format!("{id}\n")), "{out:?}");
    assert_eq!(rig.typed_count(key), 2, "a third typing");
    let inbox = rig.run(&["inbox", "--json"], &Identity::Member("api".to_owned()));
    assert_eq!(inbox.code, Some(0), "{inbox:?}");
    assert_eq!(
        inbox.stdout.matches(&format!("\"{id}\"")).count(),
        1,
        "{inbox:?}"
    );
    assert_eq!(count_with_body(&rig.store(), key), 1);
}
