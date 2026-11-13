//! `teamwire amend` through the real binary (ADR 0046): the lead changes a
//! running member's check or owns, verify uses the new values, the member
//! gets a `CHECK AMENDED:` note, and its seat rules file is rewritten.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and git in a throwaway repository"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "helper functions in an integration-test file unwrap, index and panic; a failure there fails the test"
)]

#[path = "support/amend_rig.rs"]
pub mod amend_rig;

use amend_rig::{Rig, git, role};
use teamwire_core::{HarnessId, HostId, MemberKind, MemberState, Mode};
use teamwire_store::NewMember;

#[test]
fn amend_changes_the_check_verify_runs_and_notes_the_member() {
    let rig = Rig::new();
    rig.launch("A", &["src/"], &[]);
    let run = rig.teamwire(&[
        "amend",
        "alpha",
        "--role",
        "api",
        "--check",
        "B",
        "--reason",
        "A runs every test file",
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out.contains("amended api in team alpha: check"),
        "{}",
        run.out
    );
    assert!(run.out.contains("  check: A -> B"), "{}", run.out);
    assert!(
        run.out
            .contains("run `teamwire verify alpha --role api` again"),
        "{}",
        run.out
    );

    rig.teamwire(&["verify", "alpha", "--role", "api"]);
    let log = std::fs::read_to_string(rig.home.path().join("teams/alpha/verify/api.log")).unwrap();
    assert!(
        log.lines().next().unwrap().starts_with("$ B   # in"),
        "{log}"
    );

    // The member reads the note in its inbox.
    let mut store = rig.store();
    store
        .write(|tx| tx.set_member_token_hash(&role("api"), &teamwire_store::token_hash("tok")))
        .unwrap();
    let as_api = [
        ("TEAMWIRE_TEAM", "alpha"),
        ("TEAMWIRE_ROLE", "api"),
        ("TEAMWIRE_TOKEN", "tok"),
    ];
    let inbox = rig.teamwire_with(&["inbox", "--all"], None, &as_api);
    assert_eq!(inbox.code, 0, "{}", inbox.all());
    assert!(
        inbox.out.contains("CHECK AMENDED: A runs every test file"),
        "{}",
        inbox.out
    );
    assert!(inbox.out.contains("check: A -> B"), "{}", inbox.out);
    let notes = rig.lead_notes_to("api");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0]
            .body
            .starts_with("CHECK AMENDED: A runs every test file\n")
    );
    assert!(
        notes[0].body.contains("record your result again"),
        "{}",
        notes[0].body
    );

    let log = rig.teamwire(&["log", "alpha"]);
    assert_eq!(log.code, 0, "{}", log.all());
    assert!(log.out.contains("CHECK AMENDED:"), "{}", log.out);
}

#[test]
fn amend_replaces_owns_and_keeps_a_brace_glob_whole() {
    let rig = Rig::new();
    let worktree = rig.launch("true", &["src/"], &[]);
    let run = rig.teamwire(&[
        "amend",
        "alpha",
        "--role",
        "api",
        "--owns",
        "src/",
        "--owns",
        "crates/{b,c}/**",
        "--reason",
        "api also owns b and c",
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out
            .contains("  owns: [src/] -> [src/, crates/{b,c}/**]"),
        "{}",
        run.out
    );
    assert_eq!(
        rig.api_json()["owns"],
        serde_json::json!(["src/", "crates/{b,c}/**"])
    );
    // The check did not change.
    assert_eq!(rig.member("api").check.as_deref(), Some("true"));

    std::fs::create_dir_all(worktree.join("crates/c")).unwrap();
    std::fs::write(worktree.join("crates/c/lib.rs"), "pub fn c() {}\n").unwrap();
    git(&worktree, &["add", "crates"]);
    git(&worktree, &["commit", "-q", "-m", "c"]);
    let verify = rig.teamwire(&["verify", "alpha", "--role", "api"]);
    assert!(!verify.all().contains("outside-owns"), "{}", verify.all());
}

#[test]
fn amend_refuses_without_a_change_or_a_reason_and_changes_nothing() {
    let rig = Rig::new();
    rig.launch("A", &["src/"], &[]);
    let mut store = rig.store();
    store
        .write(|tx| {
            let new = |name: &str| NewMember {
                role: role(name),
                title: name.to_owned(),
                kind: MemberKind::Implementer,
                mode: Mode::Pane,
                harness: HarnessId::from_static("claude-code"),
                host: HostId::from_static("herdr"),
                cwd: "/work".into(),
                repo: None,
                branch: None,
                base: None,
                base_sha: None,
                check: Some("A".to_owned()),
                owns: vec!["src/".to_owned()],
                deadline_s: 1800,
            };
            let at = rig.member("api").gen_started_at;
            tx.insert_member(at, &new("web"))?;
            tx.insert_member(at, &new("db"))?;
            tx.set_member_state(at, &role("db"), MemberState::Failed, &serde_json::json!({}))
        })
        .unwrap();
    let snapshot = |rig: &Rig| {
        let teams = rig.teamwire(&["teams", "--json"]).out;
        let log = rig.teamwire(&["log", "alpha"]).out;
        (teams, log)
    };
    let before = snapshot(&rig);
    let usage: [(&[&str], &str); 5] = [
        (
            &["--role", "api", "--reason", "why"],
            "required arguments were not provided",
        ),
        (
            &["--role", "api", "--check", "B"],
            "required arguments were not provided",
        ),
        (
            &["--role", "api", "--check", "B", "--reason", "  "],
            "--reason is empty",
        ),
        (
            &["--role", "api", "--check", "", "--reason", "why"],
            "--check is empty",
        ),
        (
            &["--role", "api", "--owns", "", "--reason", "why"],
            "--owns has an empty pattern",
        ),
    ];
    for (args, says) in usage {
        let mut all = vec!["amend", "alpha"];
        all.extend_from_slice(args);
        let run = rig.teamwire(&all);
        assert_eq!(run.code, 3, "{args:?}: {}", run.all());
        assert!(run.err.contains(says), "{args:?}: {}", run.err);
    }
    let refused: [(&str, &str); 3] = [
        ("ghost", "no member ghost"),
        ("web", "web is creating, not started"),
        ("db", "db is failed, not started"),
    ];
    for (name, says) in refused {
        let run = rig.teamwire(&[
            "amend", "alpha", "--role", name, "--check", "B", "--reason", "why",
        ]);
        assert_eq!(run.code, 1, "{name}: {}", run.all());
        assert!(run.err.contains(says), "{name}: {}", run.err);
    }
    let member = rig.teamwire_with(
        &[
            "amend", "alpha", "--role", "api", "--check", "B", "--reason", "why",
        ],
        None,
        &[("TEAMWIRE_ROLE", "api")],
    );
    assert_eq!(member.code, 1, "{}", member.all());
    assert!(member.err.contains("refused.identity"), "{}", member.err);
    assert_eq!(snapshot(&rig), before);

    // A removed member has no session to tell.
    rig.wait_until_settled();
    let close = rig.teamwire(&["close", "alpha", "--role", "api", "--unverified"]);
    assert_eq!(close.code, 0, "{}", close.all());
    let before = snapshot(&rig);
    let run = rig.teamwire(&[
        "amend", "alpha", "--role", "api", "--check", "B", "--reason", "why",
    ]);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("api is removed, not started"),
        "{}",
        run.err
    );
    assert_eq!(snapshot(&rig), before);

    // A team that is closing takes no amend.
    store
        .write(|tx| {
            tx.begin_close(rig.member("api").gen_started_at, teamwire_core::Pid::new(1))
                .map(|_| ())
        })
        .unwrap();
    let run = rig.teamwire(&[
        "amend", "alpha", "--role", "web", "--check", "B", "--reason", "why",
    ]);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(run.err.contains("team alpha is not open"), "{}", run.err);
    assert_eq!(rig.member("web").check.as_deref(), Some("A"));
    assert!(rig.lead_notes_to("web").is_empty());
}

#[test]
fn amend_rewrites_the_seat_rules_and_a_handoff_reads_them() {
    let rig = Rig::new();
    rig.launch("A", &["src/"], &[]);
    let contract = rig.home.path().join("teams/alpha/contract/api.md");
    let run = rig.teamwire(&[
        "amend", "alpha", "--role", "api", "--check", "B", "--reason", "why",
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    let text = std::fs::read_to_string(&contract).unwrap();
    assert!(text.contains("needs it to pass:\n    B\n"), "{text}");
    assert!(!text.contains("    A\n"), "{text}");

    rig.wait_until_settled();
    rig.record_done("handoff: test");
    let document = rig.home.path().join("teams/alpha/reports/api.handoff.md");
    std::fs::write(&document, "# api handoff\n").unwrap();
    let from = rig.herdr.calls().len();
    let handoff = rig.teamwire(&["handoff", "alpha", "--role", "api"]);
    assert_eq!(handoff.code, 0, "{}", handoff.all());
    let calls = rig.calls_since(from);
    let start = calls
        .iter()
        .find(|c| c.starts_with("agent start"))
        .unwrap_or_else(|| panic!("no agent started: {calls:?}"));
    let file = start
        .split("--append-system-prompt-file ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or_else(|| panic!("no seat rules file: {start}"));
    let seat = std::fs::read_to_string(file).unwrap();
    assert!(seat.contains("needs it to pass:\n    B\n"), "{seat}");
}

#[test]
fn amend_leaves_earlier_verify_records_unchanged() {
    let rig = Rig::new();
    rig.launch("true", &["src/"], &[]);
    rig.teamwire(&["verify", "alpha", "--role", "api"]);
    let records = |rig: &Rig| {
        rig.store()
            .read(|tx| tx.verify_records(&role("api"))?.all_or_corrupt())
            .unwrap()
    };
    let before = records(&rig);
    assert_eq!(before.len(), 1, "{before:?}");
    let run = rig.teamwire(&[
        "amend", "alpha", "--role", "api", "--check", "false", "--reason", "why",
    ]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert_eq!(records(&rig), before);
}
