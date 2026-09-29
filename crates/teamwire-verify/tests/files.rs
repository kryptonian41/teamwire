//! `--deep` and `--repair-files`: the database checks, and the readable copies
//! in `comms/` and `outbox/` (STO-10, STO-13).
#![expect(
    clippy::disallowed_methods,
    reason = "the test stands in for a crash or a person that changes a copy on disk without going through the platform"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/fixture.rs"]
pub mod fixture;

use std::path::PathBuf;

use fixture::{Fixture, Repo, implementer, start_secs};
use sha2::{Digest, Sha256};
use teamwire_core::{
    Actor, CompletionStatus, FinalSource, MessageType, Recipient, Role, Timestamp,
};
use teamwire_store::{NewCompletion, NewMessage};
use teamwire_verify::{FileProblem, VerifyOpts, check_copies, deep_check, repair_copies, run};

fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

fn now(fx: &Fixture) -> Timestamp {
    teamwire_core::Clock::now(&fx.clock)
}

/// Stores one message from `api` to the lead and returns its display id.
fn send(fx: &mut Fixture, body: &str) -> String {
    let at = now(fx);
    fx.store
        .write(|tx| {
            tx.insert_message(
                at,
                &NewMessage {
                    sender: Actor::Member(role("api")),
                    kind: MessageType::Note,
                    re: None,
                    commit: None,
                    body: body.to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Lead],
                },
            )
            .map(|stored| stored.message.display_id.to_string())
        })
        .unwrap()
}

fn team_with_member() -> (Fixture, Repo) {
    let repo = Repo::init(&[("README.md", "")], start_secs() - 10);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    fx.write_report("api", "Decisions I made: none\n");
    (fx, repo)
}

fn comms(fx: &Fixture, id: &str) -> PathBuf {
    fx.team_dir().join("comms").join(format!("{id}.md"))
}

fn write_comms(fx: &Fixture, id: &str, text: &str) {
    let path = comms(fx, id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn deep_runs_integrity_check() {
    let (mut fx, _repo) = team_with_member();
    send(&mut fx, "hello");
    let report = run(
        &mut fx.ctx(),
        None,
        VerifyOpts {
            deep: true,
            repair_files: false,
        },
    )
    .unwrap();
    let deep = report.deep.unwrap();
    assert_eq!(deep.integrity, "ok");
    assert!(deep.bad_hashes.is_empty());

    // Without `--deep` nothing is run.
    let plain = run(&mut fx.ctx(), None, VerifyOpts::default()).unwrap();
    assert!(plain.deep.is_none());
}

#[test]
fn deep_reports_a_message_body_that_no_longer_matches_its_hash() {
    let (mut fx, _repo) = team_with_member();
    let first = send(&mut fx, "first");
    let second = send(&mut fx, "second");
    fx.store
        .write(|tx| {
            tx.connection()
                .execute(
                    "UPDATE messages SET body = 'edited' WHERE display_id = ?1",
                    [&second],
                )
                .map(|_| ())
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let deep = deep_check(&fx.ctx()).unwrap();
    assert_eq!(deep.integrity, "ok");
    let bad: Vec<String> = deep.bad_hashes.iter().map(ToString::to_string).collect();
    assert_eq!(bad, [second]);
    assert_ne!(bad, [first]);
    assert!(!deep.is_clean());
    let report = run(
        &mut fx.ctx(),
        None,
        VerifyOpts {
            deep: true,
            repair_files: false,
        },
    )
    .unwrap();
    assert!(!report.is_clean(false));
}

#[test]
fn repair_rewrites_missing_copy() {
    let (mut fx, _repo) = team_with_member();
    let id = send(&mut fx, "the full text\nof the message\n");

    // Missing: found, and verify without the flag leaves it missing.
    let found = check_copies(&fx.ctx()).unwrap();
    assert_eq!(found.len(), 1);
    let finding = found.first().unwrap();
    assert_eq!(finding.path, comms(&fx, &id));
    assert_eq!(finding.problem, FileProblem::Missing);
    assert!(finding.repairable);
    let report = run(&mut fx.ctx(), None, VerifyOpts::default()).unwrap();
    assert_eq!(report.copies, found);
    assert!(!report.is_clean(false));
    assert!(!comms(&fx, &id).exists());

    // `--repair-files` writes it back, byte for byte, through the platform.
    let report = run(
        &mut fx.ctx(),
        None,
        VerifyOpts {
            deep: false,
            repair_files: true,
        },
    )
    .unwrap();
    assert_eq!(report.repaired, [comms(&fx, &id)]);
    assert!(report.copies.is_empty());
    assert!(report.is_clean(false));
    assert_eq!(
        std::fs::read_to_string(comms(&fx, &id)).unwrap(),
        "the full text\nof the message\n"
    );
    assert!(fx.platform.written().contains(&comms(&fx, &id)));

    // A copy that is already right is not written again.
    let written = fx.platform.written().len();
    assert!(repair_copies(&fx.ctx()).unwrap().is_empty());
    assert_eq!(fx.platform.written().len(), written);
}

#[test]
fn repair_rewrites_a_differing_message_copy() {
    let (mut fx, _repo) = team_with_member();
    let id = send(&mut fx, "original");
    write_comms(&fx, &id, "original but edited by hand");
    let found = check_copies(&fx.ctx()).unwrap();
    assert_eq!(found.first().unwrap().problem, FileProblem::Differs);
    assert_eq!(repair_copies(&fx.ctx()).unwrap(), [comms(&fx, &id)]);
    assert_eq!(
        std::fs::read_to_string(comms(&fx, &id)).unwrap(),
        "original"
    );
    assert!(check_copies(&fx.ctx()).unwrap().is_empty());
}

#[test]
fn a_result_file_is_checked_but_never_rewritten() {
    let (mut fx, _repo) = team_with_member();
    let at = now(&fx);
    fx.store
        .write(|tx| {
            tx.insert_completion(
                at,
                &NewCompletion {
                    role: role("api"),
                    generation: 1,
                    status: CompletionStatus::Done,
                    summary: "finished".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )?;
            tx.connection()
                .execute(
                    "UPDATE completions SET result_sha256 = ?1",
                    [Sha256::digest(b"the result").as_slice()],
                )
                .map(|_| ())
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let outbox = fx.team_dir().join("outbox").join("api.md");

    // Missing: reported, and the store cannot rebuild it, so repair leaves it.
    let found = check_copies(&fx.ctx()).unwrap();
    let finding = found.first().unwrap();
    assert_eq!(finding.path, outbox);
    assert_eq!(finding.problem, FileProblem::Missing);
    assert!(!finding.repairable);
    assert!(repair_copies(&fx.ctx()).unwrap().is_empty());
    assert!(!outbox.exists());

    // Right: no finding. Different: reported and left as found.
    std::fs::create_dir_all(outbox.parent().unwrap()).unwrap();
    std::fs::write(&outbox, "the result").unwrap();
    assert!(check_copies(&fx.ctx()).unwrap().is_empty());
    std::fs::write(&outbox, "something the member wrote").unwrap();
    let found = check_copies(&fx.ctx()).unwrap();
    assert_eq!(found.first().unwrap().problem, FileProblem::Differs);
    assert!(repair_copies(&fx.ctx()).unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(&outbox).unwrap(),
        "something the member wrote"
    );
}

#[test]
fn a_tampered_message_is_reported_and_never_written_back_as_a_copy() {
    let (mut fx, _repo) = team_with_member();
    let id = send(&mut fx, "original");
    fx.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE messages SET body = 'tampered'", [])
                .map(|_| ())
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let found = check_copies(&fx.ctx()).unwrap();
    assert!(!found.first().unwrap().repairable);
    assert!(repair_copies(&fx.ctx()).unwrap().is_empty());
    assert!(!comms(&fx, &id).exists());
}

#[test]
fn deep_reports_a_damaged_database_instead_of_an_ok() {
    use std::io::{Seek, SeekFrom, Write};
    let (mut fx, _repo) = team_with_member();
    for n in 0..40 {
        send(
            &mut fx,
            &format!(
                "message number {n} with some padding to fill pages {}",
                "x".repeat(300)
            ),
        );
    }
    // A backup is a complete database file with no write-ahead log beside it.
    let path = fx.home.path().join("damaged.db");
    fx.store.backup_to(&path).unwrap();
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    let size = file.metadata().unwrap().len();
    // Leave page 1 (the schema) alone and scribble over the middle of every other page.
    let mut at = 4096 + 200;
    while at + 64 < size {
        file.seek(SeekFrom::Start(at)).unwrap();
        file.write_all(&[0xAB; 64]).unwrap();
        at += 4096;
    }
    file.sync_all().unwrap();
    // A fresh connection reads the pages as they are on disk now.
    let mut damaged = teamwire_store::Store::open_read_only_at(&path).unwrap();
    let ctx = teamwire_store::Ctx {
        home: fx.home.path(),
        store: &mut damaged,
        registry: &fx.registry,
        platform: &fx.platform,
        clock: &fx.clock,
        config: &fx.config,
    };
    let deep = deep_check(&ctx).unwrap();
    assert!(deep.integrity.contains("Tree"), "{}", deep.integrity);
    assert!(deep.bad_hashes.is_empty());
    assert!(!deep.is_clean());
}
