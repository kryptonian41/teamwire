//! `teamwire amend` and what follows it (ADR 0046): its exit codes when the
//! note cannot be delivered or the seat rules file cannot be rewritten, the
//! wake of a finished member, and team deckh's spec-kit removal (F14) ending
//! in a `close --role` without `--unverified`.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and git in a throwaway repository"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

#[path = "support/amend_rig.rs"]
pub mod amend_rig;

use amend_rig::{Rig, git};

const OWNS_AMEND: [&str; 9] = [
    "amend", "alpha", "--role", "api", "--owns", "src/", "--owns", "lib/", "--reason",
];

fn amend_owns(rig: &Rig) -> amend_rig::Run {
    let mut args = OWNS_AMEND.to_vec();
    args.push("api also owns lib");
    rig.teamwire(&args)
}

#[test]
fn an_amend_to_a_member_with_no_live_process_exits_4_and_keeps_the_change() {
    let rig = Rig::new();
    rig.launch("true", &["src/"], &[]);
    rig.lose_process();
    let run = amend_owns(&rig);
    assert_eq!(run.code, 4, "{}", run.all());
    let lines: Vec<&str> = run.out.lines().collect();
    assert!(
        lines[0].starts_with("amended api in team alpha: owns (note m"),
        "{}",
        run.out
    );
    assert_eq!(lines[1], "  owns: [src/] -> [src/, lib/]", "{}", run.out);
    assert!(lines[2].starts_with("api: undelivered ("), "{}", run.out);
    assert!(
        run.out
            .contains("run `teamwire verify alpha --role api` again"),
        "{}",
        run.out
    );
    assert_eq!(rig.api_json()["owns"], serde_json::json!(["src/", "lib/"]));
}

#[test]
fn a_seat_rules_failure_exits_2_even_when_the_note_is_undelivered() {
    let rig = Rig::new();
    rig.launch("true", &["src/"], &[]);
    rig.lose_process();
    let contract = rig.home.path().join("teams/alpha/contract/api.md");
    std::fs::remove_file(&contract).unwrap();
    std::fs::create_dir_all(contract.join("x")).unwrap();
    let run = amend_owns(&rig);
    assert_eq!(run.code, 2, "{}", run.all());
    assert!(
        run.out
            .starts_with("amended api in team alpha: owns (note m"),
        "{}",
        run.out
    );
    assert!(run.out.contains("api: undelivered ("), "{}", run.out);
    assert!(
        run.err.contains(&contract.display().to_string()),
        "{}",
        run.err
    );
    assert!(run.err.contains("run the same amend again"), "{}", run.err);
    assert_eq!(rig.api_json()["owns"], serde_json::json!(["src/", "lib/"]));
}

#[test]
fn an_amend_note_wakes_a_done_member() {
    let rig = Rig::new();
    rig.launch("true", &["src/"], &[]);
    rig.record_done("built it");
    let before = rig.member("api").generation;
    let run = rig.teamwire(&[
        "amend", "alpha", "--role", "api", "--check", "false", "--reason", "why",
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert_eq!(rig.member("api").generation, before + 1);
    let events = rig.teamwire(&["events", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&events.out).unwrap();
    let woken: Vec<&serde_json::Value> = view["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "member.woken" && e["role"] == "api")
        .collect();
    assert_eq!(woken.len(), 1, "{}", events.out);
}

#[test]
fn the_deckh_f14_sequence_ends_with_close_role_without_unverified() {
    let rig = Rig::new();
    let worktree = rig.launch(
        "test -s specs/x/tasks.md",
        &["src/", "specs/"],
        &[("specs/x/tasks.md", "- [X] T001 build\n")],
    );
    rig.write_report(&[]);
    let verify = rig.teamwire(&["verify", "alpha", "--role", "api"]);
    assert_eq!(verify.code, 0, "{}", verify.all());
    rig.record_done("built it");

    // The spec-kit removal lands on the implementer's own branch.
    git(&worktree, &["rm", "-q", "-r", "specs/x"]);
    git(&worktree, &["commit", "-q", "-m", "remove spec-kit"]);
    let sha = git(&worktree, &["rev-parse", "HEAD"]);
    rig.write_report(&[&sha]);
    let verify = rig.teamwire(&["verify", "alpha", "--role", "api"]);
    assert_ne!(verify.code, 0, "{}", verify.all());
    assert!(verify.out.contains("FAIL"), "{}", verify.out);
    let close = rig.teamwire(&["close", "alpha", "--role", "api"]);
    assert_eq!(close.code, 1, "{}", close.all());

    let amend = rig.teamwire(&[
        "amend",
        "alpha",
        "--role",
        "api",
        "--check",
        "true",
        "--reason",
        "spec-kit removed",
    ]);
    assert_eq!(amend.code, 0, "{}", amend.all());
    rig.wait_until_settled();
    let close = rig.teamwire(&["close", "alpha", "--role", "api"]);
    assert_eq!(close.code, 1, "{}", close.all());

    let verify = rig.teamwire(&["verify", "alpha", "--role", "api"]);
    assert_eq!(verify.code, 0, "{}", verify.all());
    rig.wait_until_settled();
    let close = rig.teamwire(&["close", "alpha", "--role", "api"]);
    assert_eq!(close.code, 0, "{}", close.all());
}
