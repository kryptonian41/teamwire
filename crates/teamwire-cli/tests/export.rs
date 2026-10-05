//! `teamwire export`, run as the real binary against a team with a row in
//! every table of the schema (STO-14: every record).
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;

use std::time::Duration;

use fixture::{Bed, role};
use serde_json::Value;
use teamwire_core::{Clock, CompletionStatus, DeliveryState, HoldInfo, Pid};
use teamwire_store::{NewVerifyRecord, Waiver};

#[test]
fn export_covers_every_table() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.delivered("api", CompletionStatus::Done);
    bed.question("api", DeliveryState::HandedOff);
    let now = bed.clock.now();
    bed.store
        .write(|tx| {
            tx.insert_verify_record(
                now,
                &NewVerifyRecord {
                    role: role("api"),
                    head_sha: "abc123".to_owned(),
                    verdict: "pass".to_owned(),
                    exit_code: Some(0),
                    seconds: Some(2),
                    flags: vec![],
                    log_sha256: Some([9; 32]),
                },
            )?;
            tx.insert_waiver(
                now,
                &Waiver {
                    role: role("api"),
                    flag_id: "f1".to_owned(),
                    head_sha: "abc123".to_owned(),
                    reason: "accepted".to_owned(),
                    created_at: now,
                },
            )?;
            tx.take_watch_lease(now, Pid::new(4242), "host", |_| true)?;
            tx.announce_once(now, "key-1")?;
            tx.set_sleep_hold(Some(&HoldInfo {
                pid: Pid::new(4243),
                owned: true,
            }))?;
            tx.claim_metric_export(now, "me", Duration::from_secs(60), |_| false)?;
            Ok(())
        })
        .unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(["export", "alpha"])
        .env_remove("TEAMWIRE_TOKEN")
        .env_remove("TEAMWIRE_ROLE")
        .env_remove("TEAMWIRE_TEAM")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let records: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let has = |kind: &str| records.iter().any(|r| r["type"] == kind);
    // Each schema table, and the record type that carries it.
    for (table, kind) in [
        ("team", "teamwire.team"),
        ("members", "teamwire.member"),
        ("messages", "teamwire.message"),
        ("completions", "teamwire.completion"),
        ("verify_records", "teamwire.verify"),
        ("waivers", "teamwire.waiver"),
        ("watch_lease", "teamwire.watch_lease"),
        ("announcements", "teamwire.announcement"),
        ("counters", "teamwire.counter"),
        ("metric_export", "teamwire.metric_export"),
    ] {
        assert!(has(kind), "no {kind} record for the {table} table");
    }
    // Events carry their own event type; deliveries ride inside a message.
    assert!(records.iter().any(|r| r["seq"].is_u64()), "no event record");
    let message = records
        .iter()
        .find(|r| r["type"] == "teamwire.message")
        .unwrap();
    assert!(!message["data"]["deliveries"].as_array().unwrap().is_empty());
    let team = records
        .iter()
        .find(|r| r["type"] == "teamwire.team")
        .unwrap();
    assert_eq!(team["data"]["sleep_hold"]["pid"], 4243);
    let verify = records
        .iter()
        .find(|r| r["type"] == "teamwire.verify")
        .unwrap();
    assert_eq!(verify["data"]["head_sha"], "abc123");
}
