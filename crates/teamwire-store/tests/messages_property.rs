//! Store property (docs/test-strategy.md section 2): committed `(sender, seq)`
//! are 1..n with no gap or repeat; `(sender, key)` never makes two rows;
//! display ids never repeat; over random interleavings of send, send again
//! with the same key, and a rollback in the middle of a transaction.
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use teamwire_core::{Actor, MessageType, Recipient, Role, TeamName, Timestamp};
use teamwire_fakes::TestHome;
use teamwire_store::{MessageFilter, NewMessage, Store, StoreError};

#[derive(Debug, Clone)]
struct Send {
    sender: usize,
    key: Option<u8>,
}

#[derive(Debug, Clone)]
struct Txn {
    sends: Vec<Send>,
    /// Fail after the sends, so the transaction rolls back.
    rollback: bool,
}

fn senders() -> [Actor; 3] {
    [
        Actor::Lead,
        Actor::Member(Role::parse("api").unwrap()),
        Actor::Member(Role::parse("web").unwrap()),
    ]
}

fn txns() -> impl Strategy<Value = Vec<Txn>> {
    let send =
        (0..3usize, proptest::option::of(0..4u8)).prop_map(|(sender, key)| Send { sender, key });
    let txn = (
        proptest::collection::vec(send, 1..5),
        proptest::bool::weighted(0.3),
    )
        .prop_map(|(sends, rollback)| Txn { sends, rollback });
    proptest::collection::vec(txn, 1..12)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn seq_display_ids_and_keys_hold_over_random_interleavings(plan in txns()) {
        let home = TestHome::new().unwrap();
        let mut store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
        let actors = senders();
        // What the store must contain, tracked independently.
        let mut seen_keys: BTreeSet<(usize, u8)> = BTreeSet::new();
        let mut next_seq = [1u64; 3];
        let mut total = 0usize;
        let mut clock = 0i64;

        for txn in &plan {
            let mut keys_in_txn = seen_keys.clone();
            let mut seq_in_txn = next_seq;
            let mut made = 0usize;
            let result: Result<(), StoreError> = store.write(|tx| {
                for s in &txn.sends {
                    clock += 1;
                    let at = Timestamp::from_unix_millis(1_790_000_000_000 + clock).unwrap();
                    let stored = tx.insert_message(at, &NewMessage {
                        sender: actors[s.sender].clone(),
                        kind: MessageType::Note,
                        re: None,
                        commit: None,
                        body: format!("body {clock}"),
                        client_key: s.key.map(|k| format!("k{k}")),
                        recipients: vec![Recipient::Lead],
                    })?;
                    let is_new = match s.key {
                        Some(k) => keys_in_txn.insert((s.sender, k)),
                        None => true,
                    };
                    assert_eq!(stored.duplicate, !is_new, "duplicate flag for {s:?}");
                    if is_new {
                        made += 1;
                        assert_eq!(stored.message.seq, seq_in_txn[s.sender]);
                        seq_in_txn[s.sender] += 1;
                    }
                }
                if txn.rollback {
                    return Err(StoreError::Io("rolled back on purpose".to_owned()));
                }
                Ok(())
            });
            if txn.rollback {
                prop_assert!(result.is_err());
            } else {
                prop_assert!(result.is_ok());
                seen_keys = keys_in_txn;
                next_seq = seq_in_txn;
                total += made;
            }
        }

        let messages = store
            .read(|tx| tx.messages(&MessageFilter::default())?.all_or_corrupt())
            .unwrap();
        prop_assert_eq!(messages.len(), total);
        // Display ids are m001..m<total>, once each, in order.
        let ids: Vec<String> = messages.iter().map(|m| m.display_id.to_string()).collect();
        let expected: Vec<String> = (1..=total).map(|n| format!("m{n:03}")).collect();
        prop_assert_eq!(ids, expected);
        // Per sender, seq is exactly 1..n.
        let mut per_sender: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        for m in &messages {
            per_sender.entry(m.sender.to_string()).or_default().push(m.seq);
        }
        for (sender, mut seqs) in per_sender {
            seqs.sort_unstable();
            let n = u64::try_from(seqs.len()).unwrap();
            prop_assert_eq!(seqs, (1..=n).collect::<Vec<_>>(), "sender {}", sender);
        }
        // (sender, key) made at most one row: keyed sends match the model.
        let keyed_rows: i64 = store
            .read(|tx| Ok(tx.connection().query_row(
                "SELECT count(*) FROM messages WHERE client_key LIKE 'k_'", [], |r| r.get(0))?))
            .unwrap();
        prop_assert_eq!(usize::try_from(keyed_rows).unwrap(), seen_keys.len());
        // Events: one message.sent per committed message, none for rollbacks.
        let sent = store
            .read(|tx| tx.events_of_type(teamwire_core::EventType::MessageSent)?.all_or_corrupt())
            .unwrap();
        prop_assert_eq!(sent.len(), total);
    }
}
