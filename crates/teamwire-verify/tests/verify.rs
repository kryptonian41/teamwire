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

// ---- U2: the rules ----

/// Writes the report for `role` naming every commit since the start.
fn report_all_commits(fx: &Fixture, repo: &Repo, role: &str) {
    let base = fx.member(role).base_sha.unwrap();
    let out = repo.git(&[
        "log",
        "--reverse",
        "--format=%H",
        "--first-parent",
        "--no-merges",
        &format!("{base}..HEAD"),
    ]);
    let shas: Vec<&str> = out.lines().collect();
    fx.write_report(role, &full_report(&shas));
}

const A_TEST: &str = "#[test]\nfn a() {}\n";

/// A repository with one Rust test file, and an implementer that owns `owns`.
fn rust_member(owns: &'static [&'static str], files: &[(&str, &str)]) -> (Fixture, Repo) {
    let mut all = vec![
        ("tests/a.rs", A_TEST),
        ("Cargo.toml", "[package]\nname = \"a\"\n"),
    ];
    all.extend_from_slice(files);
    let repo = repo_on_work(&all);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), owns));
    (fx, repo)
}

#[test]
fn a_rust_branch_that_adds_ignore_is_flagged() {
    let (mut fx, repo) = rust_member(&["tests/"], &[]);
    repo.commit(
        &[("tests/a.rs", "#[test]\n#[ignore]\nfn a() {}\n")],
        "make the suite pass",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Flagged, "{record:?}");
    assert_eq!(record.exit_code, Some(0));
    assert_eq!(flag_ids(&record), ["weakened:tests/a.rs:2"]);
    let flag = record.flags.first().unwrap();
    assert!(flag.detail.contains("ignore"), "{}", flag.detail);
    assert!(!flag.waived);
}

#[test]
fn each_rust_weakening_marker_is_flagged_on_its_own_line() {
    let (mut fx, repo) = rust_member(&["src/"], &[]);
    let source = "fn a() {}\n#[allow(dead_code)]\nfn b() {}\n#![allow(unused)]\n#[should_panic]\n#[expect(clippy::all, reason = \"r\")]\nfn c() {}\n// #[ignore] in prose\n";
    repo.commit(&[("src/lib.rs", source)], "add code", start_secs() + 5);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        flag_ids(&record),
        [
            "weakened:src/lib.rs:2",
            "weakened:src/lib.rs:4",
            "weakened:src/lib.rs:5",
            "weakened:src/lib.rs:6"
        ]
    );
}

#[test]
fn a_deleted_test_file_and_a_removed_test_are_flagged() {
    let (mut fx, repo) = rust_member(
        &["tests/", "src/"],
        &[("src/lib.rs", "#[test]\nfn one() {}\n#[test]\nfn two() {}\n")],
    );
    repo.remove("tests/a.rs");
    repo.commit(
        &[("src/lib.rs", "#[test]\nfn one() {}\n")],
        "trim the suite",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        flag_ids(&record),
        ["test-deleted:tests/a.rs", "test-removed:src/lib.rs"]
    );
}

#[test]
fn a_test_moved_within_a_file_is_not_a_removal() {
    let (mut fx, repo) = rust_member(&["tests/"], &[]);
    repo.commit(
        &[("tests/a.rs", "// moved\n#[test]\nfn a() {}\n")],
        "move",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), Vec::<&str>::new());
    assert_eq!(record.verdict, Verdict::Pass);
}

#[test]
fn check_config_is_flagged_unless_the_member_owns_it() {
    for (owns, expected) in [
        (
            &["src/"][..],
            vec!["config-changed:clippy.toml", "outside-owns:clippy.toml"],
        ),
        (&["src/", "clippy.toml"][..], vec![]),
    ] {
        let (mut fx, repo) = rust_member(&["src/"], &[]);
        // Rebuild the member's owns through the store row.
        fx.store
            .write(|tx| {
                tx.connection()
                    .execute(
                        "UPDATE members SET owns = ?1",
                        [serde_json::json!(owns).to_string()],
                    )
                    .map(|_| ())
                    .map_err(teamwire_store::StoreError::from)
            })
            .unwrap();
        repo.commit(
            &[("clippy.toml", "msrv = \"1.0\"\n")],
            "tune clippy",
            start_secs() + 5,
        );
        report_all_commits(&fx, &repo, "api");
        let record = run_one(&mut fx, "api");
        assert_eq!(flag_ids(&record), expected, "owns {owns:?}");
    }
}

#[test]
fn cargo_lint_tables_are_watched_and_dependencies_are_not() {
    let manifest = "[package]\nname = \"a\"\n\n[dependencies]\nserde = \"1\"\n\n[lints.clippy]\npedantic = \"warn\"\nunwrap_used = \"warn\"\n";
    let (mut fx, repo) = rust_member(&["crates/"], &[("crates/a/Cargo.toml", manifest)]);
    let loosened = manifest
        .replace("unwrap_used = \"warn\"", "unwrap_used = \"allow\"")
        .replace("serde = \"1\"", "serde = \"2\"");
    repo.commit(
        &[("crates/a/Cargo.toml", &loosened)],
        "unblock the build",
        start_secs() + 5,
    );
    // A new crate that opts into the workspace table loosens nothing.
    repo.commit(
        &[(
            "crates/b/Cargo.toml",
            "[package]\nname = \"b\"\n\n[lints]\nworkspace = true\n",
        )],
        "add b",
        start_secs() + 6,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        flag_ids(&record),
        [
            "weakened:crates/a/Cargo.toml:9",
            "lint-removed:crates/a/Cargo.toml:9"
        ]
    );
}

#[test]
fn task_md_is_ignored_wherever_verify_looks() {
    let (mut fx, repo) = rust_member(&["src/"], &[]);
    repo.commit(
        &[("TASK.md", "# brief\n#[ignore]\n")],
        "oops",
        start_secs() + 5,
    );
    repo.write(&[("TASK.md", "# brief, edited\n")]);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), Vec::<&str>::new(), "{record:?}");
}

#[test]
fn uncommitted_paths_are_flagged() {
    let (mut fx, repo) = rust_member(&["src/"], &[]);
    repo.write(&[("src/new.rs", "fn n() {}\n"), ("TASK.md", "brief\n")]);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), ["uncommitted"]);
    assert!(record.flags.first().unwrap().detail.contains("src/new.rs"));
    assert_eq!(record.verdict, Verdict::Flagged);
}

#[test]
fn verify_changes_no_worktree() {
    let (mut fx, repo) = rust_member(&["src/"], &[]);
    repo.commit(&[("src/lib.rs", "fn a() {}\n")], "own", start_secs() + 5);
    repo.write(&[("scratch.txt", "untracked\n")]);
    report_all_commits(&fx, &repo, "api");
    let status = repo.git(&["status", "--porcelain=v2", "--untracked-files=all"]);
    let head = repo.head();
    let index = std::fs::metadata(repo.path().join(".git/index"))
        .unwrap()
        .modified()
        .unwrap();
    run_one(&mut fx, "api");
    // `git status` would rewrite the index without GIT_OPTIONAL_LOCKS=0.
    let after = std::fs::metadata(repo.path().join(".git/index"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(index, after);
    assert_eq!(
        repo.git(&["status", "--porcelain=v2", "--untracked-files=all"]),
        status
    );
    assert_eq!(repo.head(), head);
    for (cmd, _) in fx.platform.commands() {
        if cmd.program == "git" {
            let sub = cmd
                .args
                .iter()
                .skip(2)
                .map(String::as_str)
                .next()
                .unwrap_or("");
            assert!(
                ["rev-parse", "merge-base", "diff", "status", "log", "show"].contains(&sub),
                "verify ran a git command that may write: {cmd:?}"
            );
            assert!(
                cmd.env
                    .contains(&("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned()))
            );
        }
    }
}
