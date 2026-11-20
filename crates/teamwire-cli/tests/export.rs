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
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;

use std::time::Duration;

use fixture::{Bed, role};
use serde_json::Value;
use teamwire_core::{Clock, CompletionStatus, DeliveryState, HoldInfo, MemberKind, Pid};
use teamwire_store::{NewVerifyRecord, Waiver};

/// `teamwire export` with a cleared environment, so only `as_member` decides
/// whether the binary sees a member. The tests often run inside a member's
/// session, whose identity variables would otherwise reach the binary.
fn export(bed: &Bed, as_member: bool) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(bed.home.path())
        .args(["export", "alpha"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    if as_member {
        cmd.env("TEAMWIRE_TEAM", "alpha")
            .env("TEAMWIRE_ROLE", "api");
    }
    cmd.output().unwrap()
}

#[test]
fn a_member_s_export_leaves_out_the_lead_session() {
    let mut bed = Bed::new();
    bed.add("api");
    let now = bed.clock.now();
    bed.store
        .write(|tx| {
            tx.adopt_lead(
                now,
                &Bed::session("lead"),
                teamwire_core::SessionKind::Interactive,
            )
        })
        .unwrap();
    let session_of = |as_member| {
        let out = export(&bed, as_member);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .find(|r| r["type"] == "teamwire.team")
            .unwrap()["data"]["lead_session"]
            .clone()
    };
    assert!(
        session_of(false).is_string(),
        "the lead's own export keeps it"
    );
    assert!(session_of(true).is_null());
}

#[test]
fn no_line_of_a_member_s_export_holds_an_adopted_session() {
    let mut bed = Bed::new();
    bed.add("api");
    for name in ["old", "lead"] {
        let now = bed.clock.now();
        bed.store
            .write(|tx| {
                tx.adopt_lead(
                    now,
                    &Bed::session(name),
                    teamwire_core::SessionKind::Interactive,
                )
            })
            .unwrap();
    }
    let lines = |as_member| {
        let out = export(&bed, as_member);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let lead = lines(false);
    assert!(
        lead.contains("sess-old") && lead.contains("sess-lead"),
        "{lead}"
    );
    let member = lines(true);
    for line in member.lines() {
        assert!(
            !line.contains("sess-old") && !line.contains("sess-lead"),
            "{line}"
        );
    }
    let adopted: Vec<Value> = member
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|r| r["type"] == "teamwire.team.lead_adopted")
        .collect();
    assert_eq!(adopted.len(), 2, "{member}");
    for r in adopted {
        assert!(
            r["data"]["old_session"].is_null() && r["data"]["new_session"].is_null(),
            "{r}"
        );
    }
}

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
            tx.claim_metric_export(now, "me", None, Duration::from_secs(60), |_| false)?;
            Ok(())
        })
        .unwrap();

    let out = export(&bed, false);
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

/// A member record carries `quiet` in effect and the set label, so a reader
/// of an export never needs to know which kinds are quiet.
#[test]
fn a_member_record_carries_quiet_in_effect_and_the_set() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.add("hush");
    bed.add_kind("rev", MemberKind::Reviewer);
    bed.store
        .write(|tx| tx.set_member_marks(&role("hush"), true, Some("rev1")))
        .unwrap();

    let out = export(&bed, false);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let records: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let member = |name: &str| {
        let r = records
            .iter()
            .find(|r| r["type"] == "teamwire.member" && r["data"]["role"] == name)
            .unwrap();
        (r["data"]["quiet"].clone(), r["data"]["set"].clone())
    };
    assert_eq!(member("api"), (Value::Bool(false), Value::Null));
    assert_eq!(member("hush"), (Value::Bool(true), Value::from("rev1")));
    assert_eq!(member("rev"), (Value::Bool(true), Value::Null));
}

/// A recorded pid is half of the owner it names: the start time beside it is
/// exported too, for the close lock and for the metric export claim.
#[test]
fn export_writes_the_start_time_beside_each_recorded_pid() {
    let mut bed = Bed::new();
    let now = bed.clock.now();
    let started = teamwire_core::Timestamp::from_unix_millis(now.unix_millis() - 60_000).unwrap();
    bed.store
        .write(|tx| {
            tx.begin_close(
                now,
                teamwire_core::platform::ProcessRef {
                    pid: Pid::new(4244),
                    started: Some(started),
                },
            )?;
            tx.record_event(
                now,
                teamwire_core::EventType::WatchStarted,
                None,
                None,
                &serde_json::json!({}),
            )?;
            tx.claim_metric_export(
                now,
                teamwire_store::ExportHolder {
                    id: "4245".to_owned(),
                    started: Some(started),
                },
                None,
                Duration::from_secs(60),
                |_| false,
            )
        })
        .unwrap();

    let out = export(&bed, false);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let records: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let data = |kind: &str| records.iter().find(|r| r["type"] == kind).unwrap()["data"].clone();
    let team = data("teamwire.team");
    assert_eq!(team["closing_pid"], 4244);
    assert_eq!(team["closing_pid_started_at"], started.to_string());
    let mark = data("teamwire.metric_export");
    assert_eq!(mark["claimed_by"], "4245", "{mark}");
    assert_eq!(mark["claimed_by_started_at"], started.to_string());
}
