//! `verify`: the real exit code of the check, the diff base, the flags of the
//! diff rules, the five verdicts and waivers bound to a HEAD.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/fixture.rs"]
mod fixture;

use fixture::{Fixture, Repo, implementer, start_secs};
use sha2::{Digest, Sha256};
use teamwire_core::{Role, RunOutput};
use teamwire_verify::{Verdict, VerifyOpts, VerifyRecord, verify};

const T0: i64 = 0;

fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

/// A report that names every commit and has the decisions list.
fn full_report(shas: &[&str]) -> String {
    let lines: Vec<String> = shas
        .iter()
        .map(|s| format!("- {} did a thing", &s[..7]))
        .collect();
    format!("{}\n\nDecisions I made: none\n", lines.join("\n"))
}

fn run_one(fx: &mut Fixture, name: &str) -> VerifyRecord {
    let role = role(name);
    let mut records = verify(&mut fx.ctx(), Some(&role), VerifyOpts::default());
    assert_eq!(records.len(), 1);
    records.remove(0)
}

fn flag_ids(record: &VerifyRecord) -> Vec<&str> {
    record.flags.iter().map(|f| f.id.as_str()).collect()
}

/// A repository whose `main` holds `files`, with a branch `work` checked out.
fn repo_on_work(files: &[(&str, &str)]) -> Repo {
    let repo = Repo::init(files, start_secs() + T0 - 10_000);
    repo.git(&["checkout", "--quiet", "-b", "work"]);
    repo
}

// ---- U1: the check runner and the diff base ----

#[test]
fn check_exit_code_is_recorded() {
    let repo = repo_on_work(&[("src/a.rs", "fn a() {}\n")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer(
        "api",
        &repo,
        Some("echo out; echo err >&2; exit 3"),
        &["src/"],
    ));
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Fail);
    assert_eq!(record.exit_code, Some(3));

    // The check ran through the platform, in the member's directory, in `sh -c`.
    let commands = fx.platform.commands();
    let (check, timeout) = commands.iter().find(|(c, _)| c.program == "sh").unwrap();
    assert_eq!(check.args, ["-c", "echo out; echo err >&2; exit 3"]);
    assert_eq!(check.cwd.as_deref(), Some(repo.path()));
    assert_eq!(timeout.as_secs(), 1800);

    // The full output is in the log, and the store holds its hash.
    let log = record.log.unwrap();
    assert_eq!(log, fx.team_dir().join("verify").join("api.log"));
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("out\n"), "{text}");
    assert!(text.contains("err"), "{text}");
    let stored = fx
        .store
        .read(|tx| tx.verify_records(&role("api")))
        .unwrap()
        .all_or_corrupt()
        .unwrap();
    assert_eq!(stored.len(), 1);
    let stored = stored.first().unwrap();
    assert_eq!(stored.record.exit_code, Some(3));
    assert_eq!(stored.record.verdict, "FAIL");
    assert_eq!(
        stored.record.log_sha256,
        Some(<[u8; 32]>::from(Sha256::digest(text.as_bytes())))
    );
}

#[test]
fn a_timeout_is_exit_124_and_the_log_says_so() {
    let repo = repo_on_work(&[("src/a.rs", "")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("sleep 9999"), &["src/"]));
    fx.member("api");
    fx.platform.script_next(
        "sh",
        RunOutput {
            exit_code: RunOutput::TIMEOUT_EXIT,
            stdout: b"partial".to_vec(),
            stderr: Vec::new(),
            timed_out: true,
        },
    );
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Fail);
    assert_eq!(record.exit_code, Some(124));
    let text = std::fs::read_to_string(record.log.unwrap()).unwrap();
    assert!(text.contains("partial"), "{text}");
    assert!(text.contains("timed out after 1800s"), "{text}");
}

/// `main` gets a contract amendment, and the member merges it in.
fn amended_repo() -> (Repo, String) {
    let repo = Repo::init(
        &[("contract.md", "v1\n"), ("src/a.rs", "fn a() {}\n")],
        start_secs() - 10_000,
    );
    let base = repo.head();
    repo.git(&["checkout", "--quiet", "-b", "work"]);
    repo.commit(
        &[("src/b.rs", "fn b() {}\n")],
        "own work",
        start_secs() + 10,
    );
    repo.git(&["checkout", "--quiet", "main"]);
    repo.commit(
        &[("contract.md", "v2\n")],
        "amend the contract",
        start_secs() + 20,
    );
    repo.git(&["checkout", "--quiet", "work"]);
    repo.git_at(
        &["merge", "--no-ff", "--quiet", "-m", "merge main", "main"],
        start_secs() + 30,
    );
    (repo, base)
}

#[test]
fn merged_amendment_not_flagged() {
    let (repo, base) = amended_repo();
    let mut fx = Fixture::new();
    let mut setup = implementer("api", &repo, Some("true"), &["src/"]);
    setup.base_sha = Some(base);
    fx.add_member(&setup);
    let head = repo.head();
    let own = repo.git(&["log", "--format=%H", "--no-merges", "--first-parent", "-1"]);
    fx.write_report("api", &full_report(&[&own]));

    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), Vec::<&str>::new(), "{record:?}");
    assert_eq!(record.verdict, Verdict::Pass);
    assert_eq!(record.head_sha.as_deref(), Some(head.as_str()));
}

#[test]
fn without_the_forward_move_the_amendment_would_be_flagged() {
    // The same history, but the base branch is unknown: the diff starts at the
    // recorded commit, and the lead's amendment shows up as outside `owns`.
    let (repo, base) = amended_repo();
    let mut fx = Fixture::new();
    let mut setup = implementer("api", &repo, Some("true"), &["src/"]);
    setup.base_sha = Some(base);
    fx.add_member(&setup);
    fx.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET base = NULL", [])
                .map(|_| ())
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let record = run_one(&mut fx, "api");
    assert!(
        flag_ids(&record).contains(&"outside-owns:contract.md"),
        "{record:?}"
    );
}

#[test]
fn the_diff_base_never_moves_back() {
    let repo = repo_on_work(&[("contract.md", "v1\n")]);
    repo.commit(
        &[("docs/early.md", "early\n")],
        "before the start",
        start_secs() + 1,
    );
    let mut fx = Fixture::new();
    // The member starts from the second commit: the file added in it is not its own.
    let mut setup = implementer("api", &repo, Some("true"), &["src/"]);
    setup.base_sha = Some(repo.head());
    fx.add_member(&setup);
    repo.commit(&[("src/a.rs", "fn a() {}\n")], "own", start_secs() + 5);
    let own = repo.head();
    fx.write_report("api", &full_report(&[&own]));
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), Vec::<&str>::new(), "{record:?}");
}
