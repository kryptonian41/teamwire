//! `kill -9` during a send to another team's agent (ADR 0016). Both stores
//! are reopened after each kill: the recipient team's copy never exists
//! without the sender's, and neither store holds a message twice. Sending
//! each stored key again then reaches every recipient, with at most the one
//! duplicate line a kill after the typing allows.
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
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file index and unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::time::{Duration, Instant};

use rig::{Identity, Rig, all_messages, assert_integrity, count_with_body, stop_agents};
use teamwire_core::{DeliveryState, MemberKind};

const KILL_POINTS: usize = 80;
const SENDERS: usize = 4;
const RECEIVERS: usize = 5;

/// Every alpha sender to every beta receiver: 20 pairs, so no pair passes
/// the five-a-minute limit over the run.
fn pairs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in 1..=RECEIVERS {
        for s in 1..=SENDERS {
            out.push((format!("s{s}"), format!("r{r}@beta")));
        }
    }
    out
}

fn two_teams() -> Rig {
    let rig = Rig::new();
    for s in 1..=SENDERS {
        rig.add_member(&format!("s{s}"), MemberKind::Implementer, false);
    }
    rig.add_team("beta");
    for r in 1..=RECEIVERS {
        rig.add_member_to("beta", &format!("r{r}"), MemberKind::Implementer, true);
    }
    rig
}

fn send_args<'a>(to: &'a str, key: &'a str) -> Vec<&'a str> {
    vec![
        "send", "alpha", "--to", to, "--type", "finding", "--text", key, "--key", key,
    ]
}

/// Each delivery state of the message with body `key` in `store`.
fn states(store: &teamwire_store::Store, key: &str) -> Vec<DeliveryState> {
    let Some(message) = all_messages(store).into_iter().find(|m| m.body == key) else {
        return Vec::new();
    };
    store
        .read(|tx| tx.deliveries_of(&message.ulid)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .map(|d| d.delivery.state)
        .collect()
}

#[test]
fn kill9_during_cross_team_send_keeps_integrity() {
    let mut rng = rig::Rng::from_env("kill9_during_cross_team_send_keeps_integrity");
    let rig = two_teams();
    let pairs = pairs();
    let mut times: Vec<Duration> = (0..3)
        .map(|i| {
            let (from, to) = &pairs[i];
            let key = format!("warmup-{i}");
            let started = Instant::now();
            let out = rig.run(&send_args(to, &key), &Identity::Member(from.clone()));
            assert_eq!(out.code, Some(0), "{out:?}");
            started.elapsed()
        })
        .collect();
    times.sort();
    let window = times[1] * 13 / 10;
    eprintln!("kill window {window:?}");

    let mut keys = Vec::new();
    for i in 0..KILL_POINTS {
        let key = format!("cross-{i:03}");
        let delay = rng.up_to(if i % 2 == 0 { window } else { window / 3 });
        let (from, to) = &pairs[(i + 3) % pairs.len()];
        let out = rig.run_and_kill(&send_args(to, &key), &Identity::Member(from.clone()), delay);
        assert!(!out.stderr.contains("refused"), "{key}: {out:?}");
        let (alpha, beta) = (rig.store(), rig.store_of("beta"));
        assert_integrity(&alpha);
        assert_integrity(&beta);
        let (here, there) = (count_with_body(&alpha, &key), count_with_body(&beta, &key));
        assert!(
            here <= 1 && there <= 1,
            "{key}: {here} in alpha, {there} in beta"
        );
        assert!(there <= here, "{key}: beta's copy exists without alpha's");
        if there == 1 {
            let a = all_messages(&alpha)
                .into_iter()
                .find(|m| m.body == key)
                .unwrap();
            let b = all_messages(&beta)
                .into_iter()
                .find(|m| m.body == key)
                .unwrap();
            assert_eq!(a.ulid, b.ulid, "{key}: the two copies have different ids");
        }
        if here == 1 {
            keys.push((key, from.clone(), to.clone()));
        }
    }
    eprintln!(
        "{} of {KILL_POINTS} sends had a row at the kill",
        keys.len()
    );
    assert!(keys.len() >= KILL_POINTS / 10, "the kill window is off");

    // A resume skips a `pending` row whose attempt began under a minute ago,
    // since its sender may still be typing. These senders are dead, so their
    // attempts are moved back to long ago, as if the minute had passed.
    rig.store()
        .write(|tx| {
            tx.connection()
                .execute(
                    "UPDATE messages SET created_at = ?1",
                    [rig::at().to_string()],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    // Sending each stored key again reaches the recipient and makes no row.
    for (key, from, to) in &keys {
        let out = rig.run(&send_args(to, key), &Identity::Member(from.clone()));
        assert!(!out.stderr.contains("refused"), "{key}: {out:?}");
        let (alpha, beta) = (rig.store(), rig.store_of("beta"));
        assert_eq!(count_with_body(&alpha, key), 1, "{key}");
        assert_eq!(count_with_body(&beta, key), 1, "{key}");
        let reached = |s: &[DeliveryState]| {
            s.iter()
                .all(|d| matches!(d, DeliveryState::HandedOff | DeliveryState::Unconfirmed))
        };
        assert!(
            reached(&states(&alpha, key)),
            "{key}: {:?}",
            states(&alpha, key)
        );
        assert!(
            reached(&states(&beta, key)),
            "{key}: {:?}",
            states(&beta, key)
        );
        let typed = rig.typed_count(key);
        assert!((1..=2).contains(&typed), "{key} typed {typed} times");
    }
    stop_agents(&rig);
}
