//! `verify`: the real exit code of the check, the diff base, the flags of the
//! diff rules, the five verdicts, and waivers: bound to a HEAD, except that a
//! file flag's waiver holds while that file is unchanged (ADR 0045).
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/fixture.rs"]
pub mod fixture;

use fixture::{Fixture, Repo, implementer, implementer_without_repo, start_secs};
use sha2::{Digest, Sha256};
use teamwire_core::{CompletionStatus, FinalSource, Role, RunOutput};
use teamwire_store::NewCompletion;
use teamwire_verify::{FlagId, Verdict, VerifyError, VerifyOpts, VerifyRecord, run, waive};

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
    let mut records = run(&mut fx.ctx(), Some(&role), VerifyOpts::default())
        .unwrap()
        .records;
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

/// The spec member amends `spec.md` on the spec branch (`main` here)
/// before the implementer's first commit, and the implementer merges the
/// amendment with `merge_args`, then commits its own work. Returns the
/// amendment's sha and the verify record.
fn merge_a_spec_amendment(merge_args: &[&str]) -> (String, VerifyRecord) {
    let repo = repo_on_work(&[
        ("specs/001-x/spec.md", "v1\n"),
        ("specs/001-x/tasks.md", "- [ ] T001\n"),
    ]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer(
        "api",
        &repo,
        Some("true"),
        &["src/", "specs/001-x/tasks.md"],
    ));
    repo.git(&["checkout", "--quiet", "main"]);
    let amendment = repo.commit(
        &[("specs/001-x/spec.md", "v2\n")],
        "SPEC AMENDMENT: clarify",
        start_secs() + 10,
    );
    repo.git(&["checkout", "--quiet", "work"]);
    let mut args = vec!["merge"];
    args.extend_from_slice(merge_args);
    args.extend(["--quiet", "--no-edit", "main"]);
    repo.git_at(&args, start_secs() + 15);
    let own = repo.commit(
        &[
            ("src/a.rs", "fn a() {}\n"),
            ("specs/001-x/tasks.md", "- [x] T001\n"),
        ],
        "T001",
        start_secs() + 20,
    );
    fx.write_report("api", &full_report(&[&own]));
    (amendment, run_one(&mut fx, "api"))
}

/// Without `--no-ff` the merge fast-forwards, so the amendment sits on the
/// implementer's own first-parent chain: verify counts it as the member's
/// own commit, outside `owns` and missing from the report.
#[test]
fn a_spec_amendment_merge_that_fast_forwards_is_flagged() {
    let (amendment, record) = merge_a_spec_amendment(&[]);
    assert_eq!(
        flag_ids(&record),
        [
            "outside-owns:specs/001-x/spec.md".to_owned(),
            format!("report-no-line:{}", &amendment[..7]),
        ],
        "{record:?}"
    );
}

/// The notice's `git merge --no-ff --no-edit <sha>` always makes a merge
/// commit, so the diff start moves to the amendment and no flag is raised.
#[test]
fn a_spec_amendment_merged_with_no_ff_is_not_flagged() {
    let (_, record) = merge_a_spec_amendment(&["--no-ff"]);
    assert_eq!(flag_ids(&record), Vec::<&str>::new(), "{record:?}");
    assert_eq!(record.verdict, Verdict::Pass);
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

/// spec-kit's `analyze` and `converge` treat the constitution as the
/// authority over the code, so a change to it changes what is checked.
#[test]
fn the_spec_kit_constitution_is_check_config() {
    const PATH: &str = ".specify/memory/constitution.md";
    for (owns, expected) in [
        (
            &["src/"][..],
            vec![
                "config-changed:.specify/memory/constitution.md",
                "outside-owns:.specify/memory/constitution.md",
            ],
        ),
        (&["src/", ".specify/"][..], vec![]),
    ] {
        let repo = repo_on_work(&[(PATH, "Every feature has a test.\n")]);
        let mut fx = Fixture::new();
        fx.add_member(&implementer("api", &repo, Some("true"), owns));
        repo.commit(
            &[(PATH, "Tests are optional.\n")],
            "relax",
            start_secs() + 5,
        );
        report_all_commits(&fx, &repo, "api");
        let record = run_one(&mut fx, "api");
        assert_eq!(flag_ids(&record), expected, "owns {owns:?}");
    }
}

// ---- ADR 0045: a waiver for a file holds until that file changes ----

const CONSTITUTION: &str = ".specify/memory/constitution.md";
const CONSTITUTION_FLAGS: [&str; 2] = [
    "config-changed:.specify/memory/constitution.md",
    "outside-owns:.specify/memory/constitution.md",
];

/// The deckh replay up to the waivers: a member that owns `src/` changes the
/// constitution, and the lead waives both flags at that HEAD.
fn constitution_waived() -> (Fixture, Repo) {
    let repo = repo_on_work(&[
        (CONSTITUTION, "Every feature has a test.\n"),
        ("src/a.rs", "fn a() {}\n"),
    ]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    repo.commit(
        &[(CONSTITUTION, "Every feature has a test, and a doc.\n")],
        "amend the constitution",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    assert_eq!(flag_ids(&run_one(&mut fx, "api")), CONSTITUTION_FLAGS);
    for flag in CONSTITUTION_FLAGS {
        waive(
            &mut fx.ctx(),
            &role("api"),
            &FlagId::new(flag),
            "the user asked for this amendment",
        )
        .unwrap();
    }
    assert_eq!(run_one(&mut fx, "api").verdict, Verdict::Pass);
    (fx, repo)
}

/// Each named flag of the record with whether it is waived.
fn waived_of<'a>(record: &'a VerifyRecord, ids: &[&str]) -> Vec<(&'a str, bool)> {
    record
        .flags
        .iter()
        .filter(|f| ids.contains(&f.id.as_str()))
        .map(|f| (f.id.as_str(), f.waived))
        .collect()
}

#[test]
fn a_file_flag_waiver_holds_while_that_file_is_unchanged() {
    let (mut fx, repo) = constitution_waived();
    repo.commit(&[("src/b.rs", "fn b() {}\n")], "own work", start_secs() + 9);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Pass, "{record:?}");
    assert_eq!(
        waived_of(&record, &CONSTITUTION_FLAGS),
        CONSTITUTION_FLAGS.map(|id| (id, true))
    );
}

#[test]
fn a_waiver_for_a_deleted_file_holds_while_the_file_stays_deleted() {
    let (mut fx, repo) = deleted_test();
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("test-deleted:tests/a.rs"),
        "the suite moved to src/",
    )
    .unwrap();
    repo.commit(&[("src/more.rs", "fn m() {}\n")], "more", start_secs() + 9);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        waived_of(&record, &["test-deleted:tests/a.rs"]),
        [("test-deleted:tests/a.rs", true)],
        "{record:?}"
    );
}

#[test]
fn a_file_flag_waiver_ends_when_that_file_changes() {
    let (mut fx, repo) = constitution_waived();
    repo.commit(
        &[(CONSTITUTION, "Tests are optional.\n")],
        "relax",
        start_secs() + 9,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Flagged, "{record:?}");
    assert_eq!(
        waived_of(&record, &CONSTITUTION_FLAGS),
        CONSTITUTION_FLAGS.map(|id| (id, false))
    );
}

#[test]
fn a_file_flag_waiver_ends_when_a_merge_changes_that_file() {
    let (mut fx, repo) = constitution_waived();
    repo.git(&["checkout", "--quiet", "-b", "side"]);
    repo.commit(
        &[(CONSTITUTION, "Tests are optional.\n")],
        "relax on a side branch",
        start_secs() + 7,
    );
    repo.git(&["checkout", "--quiet", "work"]);
    repo.git_at(
        &["merge", "--no-ff", "--quiet", "-m", "merge side", "side"],
        start_secs() + 9,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        waived_of(&record, &CONSTITUTION_FLAGS),
        CONSTITUTION_FLAGS.map(|id| (id, false)),
        "{record:?}"
    );
}

#[test]
fn a_waiver_at_a_commit_git_does_not_know_does_not_carry_over() {
    let repo = repo_on_work(&[(CONSTITUTION, "Every feature has a test.\n")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    repo.commit(
        &[(CONSTITUTION, "Tests are optional.\n")],
        "relax",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    let at = teamwire_core::Clock::now(&fx.clock);
    fx.store
        .write(|tx| {
            tx.insert_waiver(
                at,
                &teamwire_store::Waiver {
                    role: role("api"),
                    flag_id: CONSTITUTION_FLAGS[0].to_owned(),
                    head_sha: "0".repeat(40),
                    reason: "recorded before a rebase".to_owned(),
                    created_at: at,
                },
            )
            .map(|_| ())
        })
        .unwrap();
    let record = run_one(&mut fx, "api");
    assert_eq!(
        waived_of(&record, &CONSTITUTION_FLAGS[..1]),
        [(CONSTITUTION_FLAGS[0], false)],
        "{record:?}"
    );
}

#[test]
fn a_waiver_for_a_flag_with_no_file_ends_at_the_next_commit() {
    let repo = repo_on_work(&[("src/lib.rs", "fn a() {}\n"), ("billing/plan.rs", "x\n")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &[]));
    repo.commit(
        &[("billing/plan.rs", "y\n")],
        "touch billing",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    assert_eq!(flag_ids(&run_one(&mut fx, "api")), ["no-owns"]);
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("no-owns"),
        "reads only",
    )
    .unwrap();
    repo.commit(
        &[("billing/plan.rs", "z\n")],
        "touch again",
        start_secs() + 9,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        waived_of(&record, &["no-owns"]),
        [("no-owns", false)],
        "{record:?}"
    );
}

#[test]
fn an_uncommitted_path_waiver_ends_at_the_next_commit() {
    let repo = repo_on_work(&[("src/lib.rs", "fn a() {}\n")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    report_all_commits(&fx, &repo, "api");
    repo.write(&[("notes.txt", "scratch\n")]);
    assert_eq!(
        flag_ids(&run_one(&mut fx, "api")),
        ["uncommitted:notes.txt"]
    );
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("uncommitted:notes.txt"),
        "scratch file",
    )
    .unwrap();
    // Commit another file only, so notes.txt stays uncommitted.
    repo.write(&[("src/b.rs", "fn b() {}\n")]);
    repo.git(&["add", "--", "src/b.rs"]);
    repo.git_at(&["commit", "--quiet", "-m", "own work"], start_secs() + 9);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(
        waived_of(&record, &["uncommitted:notes.txt"]),
        [("uncommitted:notes.txt", false)],
        "{record:?}"
    );
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
    assert_eq!(flag_ids(&record), ["uncommitted:src/new.rs"]);
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
                [
                    "rev-parse",
                    "merge-base",
                    "diff-tree",
                    "status",
                    "log",
                    "show"
                ]
                .contains(&sub),
                "verify ran a git command that may write: {cmd:?}"
            );
            assert!(
                cmd.env
                    .contains(&("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned()))
            );
        }
    }
}

// ---- U3: verdicts, records, waivers ----

fn stored(fx: &Fixture, name: &str) -> Vec<teamwire_store::VerifyRow> {
    fx.store
        .read(|tx| tx.verify_records(&role(name)))
        .unwrap()
        .all_or_corrupt()
        .unwrap()
}

/// A member that deleted its only test and whose check exits 0.
fn deleted_test() -> (Fixture, Repo) {
    let (fx, repo) = rust_member(&["src/", "tests/"], &[]);
    repo.remove("tests/a.rs");
    repo.commit(
        &[("src/lib.rs", "fn a() {}\n")],
        "green build",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    (fx, repo)
}

#[test]
fn flags_with_exit_zero_is_flagged_not_pass() {
    let (mut fx, repo) = deleted_test();
    let record = run_one(&mut fx, "api");
    assert_eq!(record.exit_code, Some(0));
    assert_eq!(record.verdict, Verdict::Flagged, "{record:?}");
    assert_eq!(flag_ids(&record), ["test-deleted:tests/a.rs"]);
    let rows = stored(&fx, "api");
    let row = rows.first().unwrap();
    assert_eq!(row.record.verdict, "FLAGGED");
    assert_eq!(row.record.head_sha, repo.head());
    assert_eq!(row.record.flags, ["test-deleted:tests/a.rs"]);
}

fn record_done(fx: &mut Fixture, name: &str) {
    let at = teamwire_core::Clock::now(&fx.clock);
    fx.store
        .write(|tx| {
            tx.insert_completion(
                at,
                &NewCompletion {
                    role: role(name),
                    generation: 1,
                    status: CompletionStatus::Done,
                    summary: "finished".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
            .map(|_| ())
        })
        .unwrap();
}

#[test]
fn a_done_implementer_with_no_commit_is_flagged_and_a_waiver_clears_it() {
    let (mut fx, repo) = rust_member(&["src/"], &[]);
    report_all_commits(&fx, &repo, "api");
    assert_eq!(flag_ids(&run_one(&mut fx, "api")), Vec::<&str>::new());

    record_done(&mut fx, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), ["no-commits"], "{record:?}");
    assert_eq!(record.verdict, Verdict::Flagged);

    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("no-commits"),
        "nothing to change",
    )
    .unwrap();
    assert_eq!(run_one(&mut fx, "api").verdict, Verdict::Pass);

    repo.commit(&[("src/a.rs", "fn a() {}\n")], "work", start_secs() + 5);
    report_all_commits(&fx, &repo, "api");
    assert_eq!(flag_ids(&run_one(&mut fx, "api")), Vec::<&str>::new());
}

#[test]
fn a_waiver_makes_the_flagged_pass_a_pass_for_that_head_only() {
    let (mut fx, repo) = deleted_test();
    let flag = FlagId::new("test-deleted:tests/a.rs");
    waive(
        &mut fx.ctx(),
        &role("api"),
        &flag,
        "the suite moved to src/",
    )
    .unwrap();

    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Pass, "{record:?}");
    assert!(record.flags.first().unwrap().waived);
    assert_eq!(stored(&fx, "api").last().unwrap().record.verdict, "PASS");
    let waivers = fx
        .store
        .read(|tx| tx.all_waivers())
        .unwrap()
        .all_or_corrupt()
        .unwrap();
    let waiver = waivers.first().unwrap();
    assert_eq!(waiver.flag_id, "test-deleted:tests/a.rs");
    assert_eq!(waiver.head_sha, repo.head());
    assert_eq!(waiver.reason, "the suite moved to src/");

    // One more commit moves HEAD and leaves tests/a.rs deleted: the waiver
    // names that one file, so it carries over (ADR 0045).
    repo.commit(&[("src/more.rs", "fn m() {}\n")], "more", start_secs() + 9);
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Pass, "{record:?}");
    assert!(record.flags.first().unwrap().waived);
}

#[test]
fn a_waiver_names_one_flag_and_one_flag_only() {
    let (mut fx, repo) = rust_member(&["src/"], &[]);
    repo.commit(
        &[(
            "src/lib.rs",
            "#[allow(dead_code)]\nfn a() {}\n#[allow(unused)]\nfn b() {}\n",
        )],
        "add",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("weakened:src/lib.rs:1"),
        "dead code is on purpose",
    )
    .unwrap();
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Flagged);
    let waived: Vec<(&str, bool)> = record
        .flags
        .iter()
        .map(|f| (f.id.as_str(), f.waived))
        .collect();
    assert_eq!(
        waived,
        [
            ("weakened:src/lib.rs:1", true),
            ("weakened:src/lib.rs:3", false)
        ]
    );
}

#[test]
fn waiving_a_flag_the_member_does_not_have_is_refused() {
    let (mut fx, _repo) = deleted_test();
    let ghost = FlagId::new("test-deleted:tests/b.rs");
    let err = waive(&mut fx.ctx(), &role("api"), &ghost, "why").unwrap_err();
    assert_eq!(err, VerifyError::UnknownFlag(ghost));
    let err = waive(&mut fx.ctx(), &role("nobody"), &FlagId::new("x"), "why").unwrap_err();
    assert_eq!(err, VerifyError::UnknownRole(role("nobody")));
    let err = waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("test-deleted:tests/a.rs"),
        "  ",
    )
    .unwrap_err();
    assert!(matches!(err, VerifyError::Internal(_)), "{err:?}");
    assert!(
        fx.store
            .read(|tx| tx.all_waivers())
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn a_failing_check_stays_fail_even_when_every_flag_is_waived() {
    let (mut fx, _repo) = deleted_test();
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("test-deleted:tests/a.rs"),
        "ok",
    )
    .unwrap();
    fx.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET check_cmd = 'exit 1'", [])
                .map(|_| ())
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Fail);
    assert_eq!(record.exit_code, Some(1));
}

#[test]
fn a_worktree_member_with_no_check_is_no_check_and_never_clean() {
    let repo = repo_on_work(&[("src/a.rs", "")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, None, &["src/"]));
    fx.write_report("api", "Decisions I made: none\n");
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::NoCheck);
    assert_eq!(record.exit_code, None);
    assert_eq!(
        stored(&fx, "api").last().unwrap().record.verdict,
        "NO-CHECK"
    );
}

#[test]
fn removed_unstarted_and_checkless_pane_members_are_skipped() {
    let mut fx = Fixture::new();
    let cwd = fx.home.path().to_path_buf();
    fx.add_member(&implementer_without_repo(
        "gone",
        &cwd,
        Some("true"),
        &["src/"],
    ));
    let now = teamwire_core::Clock::now(&fx.clock);
    fx.store
        .write(|tx| tx.remove_member(now, &role("gone"), Some("done"), &serde_json::json!({})))
        .unwrap();
    let mut pane = implementer_without_repo("pane", &cwd, None, &[]);
    pane.mode = teamwire_core::Mode::Pane;
    pane.kind = teamwire_core::MemberKind::Reviewer;
    fx.add_member(&pane);
    let records = run(&mut fx.ctx(), None, VerifyOpts::default())
        .unwrap()
        .records;
    let verdicts: Vec<(&str, Verdict)> = records
        .iter()
        .map(|r| (r.role.as_str(), r.verdict))
        .collect();
    assert_eq!(
        verdicts,
        [("gone", Verdict::Skipped), ("pane", Verdict::Skipped)]
    );
    assert!(
        records
            .iter()
            .all(|r| r.note.is_some() && r.head_sha.is_none())
    );
    assert!(stored(&fx, "gone").is_empty());
    assert!(
        fx.platform.commands().is_empty(),
        "a skipped member runs nothing"
    );
}

#[test]
fn a_pane_member_with_a_check_is_checked_without_the_diff_rules() {
    let dir = tempfile::tempdir().unwrap();
    let mut fx = Fixture::new();
    let mut pane = implementer_without_repo("pane", dir.path(), Some("true"), &["src/"]);
    pane.mode = teamwire_core::Mode::Pane;
    pane.kind = teamwire_core::MemberKind::Reviewer;
    fx.add_member(&pane);
    let record = run_one(&mut fx, "pane");
    assert_eq!(record.verdict, Verdict::Pass);
    assert!(record.flags.is_empty());
    assert_eq!(record.head_sha, None);
    assert_eq!(stored(&fx, "pane").last().unwrap().record.head_sha, "none");
}

#[test]
fn an_unknown_role_is_an_error_and_records_nothing() {
    let mut fx = Fixture::new();
    let err = run(&mut fx.ctx(), Some(&role("nobody")), VerifyOpts::default()).unwrap_err();
    assert_eq!(err, VerifyError::UnknownRole(role("nobody")));
}

#[test]
fn the_report_is_clean_only_when_every_checked_member_passes() {
    let repo = repo_on_work(&[("src/a.rs", "")]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    fx.write_report("api", "Decisions I made: none\n");
    let report = run(&mut fx.ctx(), None, VerifyOpts::default()).unwrap();
    assert!(report.is_clean(false));
    fx.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET check_cmd = 'exit 2'", [])
                .map(|_| ())
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let report = run(&mut fx.ctx(), None, VerifyOpts::default()).unwrap();
    assert!(!report.is_clean(false));
}

// ---- the lead merges the member's branch into its base ----

/// A member deletes a test and edits outside its owns, and commits.
fn member_with_flags() -> (Fixture, Repo) {
    let (fx, repo) = rust_member(&["src/"], &[]);
    repo.remove("tests/a.rs");
    repo.commit(&[("outside.txt", "x\n")], "own work", start_secs() + 5);
    (fx, repo)
}

fn assert_flags_kept(fx: &mut Fixture, repo: &Repo) {
    report_all_commits(fx, repo, "api");
    let record = run_one(fx, "api");
    let ids = flag_ids(&record);
    assert!(ids.contains(&"outside-owns:outside.txt"), "{record:?}");
    assert!(
        ids.iter().any(|f| f.starts_with("test-deleted")),
        "{record:?}"
    );
}

#[test]
fn lead_merge_into_base_does_not_hide_flags() {
    let (mut fx, repo) = member_with_flags();
    repo.git(&["checkout", "--quiet", "main"]);
    repo.git_at(
        &["merge", "--no-ff", "--quiet", "-m", "integrate", "work"],
        start_secs() + 50,
    );
    repo.git(&["checkout", "--quiet", "work"]);
    assert_flags_kept(&mut fx, &repo);
}

#[test]
fn fast_forward_of_base_does_not_hide_flags() {
    let (mut fx, repo) = member_with_flags();
    repo.git(&["branch", "--force", "main", "HEAD"]);
    assert_flags_kept(&mut fx, &repo);
}

// ---- skip forms and dropped asserts (VER-3) ----

const LIB: &str = "pub fn a() -> u32 { 1 }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn one() {\n        assert_eq!(a(), 1);\n    }\n}\n";
const IT: &str = "#[test]\nfn it_one() {\n    assert_eq!(1, 1);\n}\n\n#[test]\nfn it_two() {\n    assert_eq!(2, 2);\n}\n";
const JS: &str = "const { it } = require('node:test');\n\nit('adds', () => {\n  check();\n});\n";
const PY: &str = "def test_one():\n    assert f() == 1\n";
const GO: &str = "package a\n\nimport \"testing\"\n\nfunc TestOne(t *testing.T) {\n\tif f() != 1 {\n\t\tt.Fatal(\"no\")\n\t}\n}\n";

/// The flags of the diff rules after one commit that rewrites `path`.
fn flags_after_edit(path: &str, text: &str) -> Vec<String> {
    let repo = repo_on_work(&[
        ("src/lib.rs", LIB),
        ("tests/it.rs", IT),
        ("test/a.test.js", JS),
        ("tests/test_a.py", PY),
        ("a_test.go", GO),
    ]);
    repo.commit(&[(path, text)], "change", start_secs() + 10);
    let mut fx = Fixture::new();
    fx.add_member(&implementer(
        "api",
        &repo,
        Some("true"),
        &["src/", "tests/", "test/", "a_test.go"],
    ));
    let record = run_one(&mut fx, "api");
    record
        .flags
        .iter()
        .map(|f| f.id.as_str().to_owned())
        .filter(|id| id.starts_with("weakened:") || id.starts_with("asserts-removed:"))
        .collect()
}

// Not flagged, by design (D17): `#[cfg(not(test))]` (the rules have no path
// filter, so it would flag production code), a test wrapped in `/* */`, and
// a net-zero swap of one test for another. Also not flagged: `it.skip.each`,
// `it.skipIf(` and the focus form `fit(`.
#[test]
fn every_added_skip_form_is_flagged_end_to_end() {
    let it_ignore = |attr: &str| {
        IT.replacen(
            "#[test]\nfn it_one",
            &format!("#[test]\n{attr}\nfn it_one"),
            1,
        )
    };
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "tests/it.rs",
            it_ignore("#[ignore]"),
            "weakened:tests/it.rs:2",
        ),
        (
            "tests/it.rs",
            it_ignore("# [ignore]"),
            "weakened:tests/it.rs:2",
        ),
        (
            "tests/it.rs",
            it_ignore("#[ ignore ]"),
            "weakened:tests/it.rs:2",
        ),
        (
            "tests/it.rs",
            format!("#![cfg(any())]\n{IT}"),
            "weakened:tests/it.rs:1",
        ),
        (
            "tests/it.rs",
            IT.replace("assert_eq!", "let _ = "),
            "asserts-removed:tests/it.rs",
        ),
        (
            "test/a.test.js",
            JS.replace("it('adds'", "it.skip('adds'"),
            "weakened:test/a.test.js:3",
        ),
        (
            "test/a.test.js",
            JS.replace("it('adds', () =>", "it('adds', { skip: true }, () =>"),
            "weakened:test/a.test.js:3",
        ),
        (
            "tests/test_a.py",
            format!("import pytest\npytestmark = pytest.mark.skip(reason='x')\n{PY}"),
            "weakened:tests/test_a.py:2",
        ),
        (
            "tests/test_a.py",
            PY.replace(
                "def test_one():\n",
                "def test_one():\n    raise unittest.SkipTest('x')\n",
            ),
            "weakened:tests/test_a.py:2",
        ),
        (
            "a_test.go",
            GO.replace("\tif f()", "\tt.Skipf(\"later %d\", 1)\n\tif f()"),
            "weakened:a_test.go:6",
        ),
        (
            "a_test.go",
            GO.replace("\tif f()", "\tt.Skip(\"later\")\n\tif f()"),
            "weakened:a_test.go:6",
        ),
    ];
    for (path, text, expected) in cases {
        assert_eq!(flags_after_edit(path, &text), [expected], "{path}");
    }
}

#[test]
fn a_commit_that_removes_two_asserts_from_a_test_file_is_flagged() {
    let before =
        "#[test]\nfn a() {\n    assert_eq!(1, 1);\n    assert_eq!(2, 2);\n    assert!(true);\n}\n";
    let (mut fx, repo) = rust_member(&["tests/"], &[("tests/b.rs", before)]);
    repo.commit(
        &[("tests/b.rs", "#[test]\nfn a() {\n    assert!(true);\n}\n")],
        "simplify",
        start_secs() + 5,
    );
    report_all_commits(&fx, &repo, "api");
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), ["asserts-removed:tests/b.rs"]);
    assert_eq!(record.verdict, Verdict::Flagged);
    assert!(
        record
            .flags
            .first()
            .unwrap()
            .detail
            .contains("2 assert line(s) removed and 0 added")
    );
}

#[test]
fn a_waiver_of_one_uncommitted_path_does_not_cover_a_later_edit_of_another() {
    let (mut fx, repo) = rust_member(&["src/", "tests/"], &[]);
    report_all_commits(&fx, &repo, "api");
    repo.write(&[("notes.txt", "scratch\n")]);
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), ["uncommitted:notes.txt"]);
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("uncommitted:notes.txt"),
        "scratch file",
    )
    .unwrap();
    assert_eq!(run_one(&mut fx, "api").verdict, Verdict::Pass);

    // Later, at the same HEAD, another file changes.
    repo.write(&[("tests/a.rs", "#[test]\n#[ignore]\nfn a() {}\n")]);
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Flagged, "{record:?}");
    let open: Vec<&str> = record
        .flags
        .iter()
        .filter(|f| !f.waived)
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(open, ["uncommitted:tests/a.rs"]);
}

#[test]
fn a_started_worktree_member_whose_folder_is_gone_fails() {
    let repo = repo_on_work(&[("src/lib.rs", LIB)]);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    std::fs::remove_dir_all(repo.path()).unwrap();
    let report = run(&mut fx.ctx(), None, VerifyOpts::default()).unwrap();
    let record = report.records.first().unwrap();
    assert_eq!(record.verdict, Verdict::Fail);
    assert!(record.note.as_deref().unwrap().contains("does not exist"));
    assert!(!report.is_clean(false));
}

#[test]
fn an_implementer_with_no_owns_list_is_flagged_and_a_waiver_clears_it() {
    let repo = repo_on_work(&[("src/lib.rs", LIB), ("billing/plan.rs", "x\n")]);
    let sha = repo.commit(
        &[("billing/plan.rs", "y\n")],
        "touch billing",
        start_secs() + 10,
    );
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &[]));
    fx.write_report("api", &full_report(&[&sha]));
    let record = run_one(&mut fx, "api");
    assert_eq!(flag_ids(&record), ["no-owns"]);
    assert_eq!(record.verdict, Verdict::Flagged);
    waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("no-owns"),
        "reads only",
    )
    .unwrap();
    assert_eq!(run_one(&mut fx, "api").verdict, Verdict::Pass);
}

#[test]
fn a_commit_made_during_the_check_flags_head_moved_and_no_waiver_clears_it() {
    let repo = repo_on_work(&[("src/lib.rs", LIB)]);
    let first = repo.head();
    let git_dir = repo.path().display();
    let check =
        format!("git -C {git_dir} -c user.name=t -c user.email=t@t commit --allow-empty -qm moved");
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some(&check), &["src/"]));
    let record = run_one(&mut fx, "api");
    let moved = repo.head();
    assert_ne!(first, moved);
    assert_eq!(record.verdict, Verdict::Flagged, "{record:?}");
    assert_eq!(record.head_sha.as_deref(), Some(first.as_str()));
    let flag = record
        .flags
        .iter()
        .find(|f| f.id.as_str() == "head-moved")
        .unwrap();
    assert_eq!(
        flag.detail,
        format!("HEAD moved from {first} to {moved} while verify ran; run verify again")
    );
    assert!(!flag.waived);
    let waived = waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("head-moved"),
        "ok",
    );
    assert!(
        matches!(waived, Err(VerifyError::UnknownFlag(_))),
        "{waived:?}"
    );
}

#[test]
fn an_amend_while_the_check_runs_flags_scope_changed() {
    let repo = repo_on_work(&[("src/lib.rs", LIB)]);
    let sha = repo.commit(&[("src/b.rs", "x\n")], "add b", start_secs() + 10);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    fx.write_report("api", &full_report(&[&sha]));
    // The lead amends the member from another process while its check runs.
    let home = fx.home.path().to_path_buf();
    fx.platform.before_next_run("sh", move || {
        let team = teamwire_core::TeamName::parse(Fixture::TEAM).unwrap();
        let mut other = teamwire_store::Store::open(&home, &team).unwrap();
        other
            .write(|tx| tx.set_member_scope(&role("api"), Some("false"), &["src/".to_owned()]))
            .unwrap();
    });
    let record = run_one(&mut fx, "api");
    assert_eq!(record.verdict, Verdict::Flagged, "{record:?}");
    let flag = record
        .flags
        .iter()
        .find(|f| f.id.as_str() == "scope-changed")
        .unwrap();
    assert_eq!(
        flag.detail,
        "the lead amended this member's check or owns while verify ran; run verify again"
    );
    assert!(!flag.waived);
    // The stored row says the same as the printed record.
    let stored = fx
        .store
        .read(|tx| tx.verify_records(&role("api")))
        .unwrap()
        .all_or_corrupt()
        .unwrap();
    let stored = stored.last().unwrap();
    assert_eq!(stored.record.verdict, "FLAGGED");
    assert!(
        stored.record.flags.iter().any(|f| f == "scope-changed"),
        "{:?}",
        stored.record.flags
    );
    // The next run uses the amended check and raises no such flag.
    let again = run_one(&mut fx, "api");
    assert_eq!(again.verdict, Verdict::Fail, "{again:?}");
    assert!(!flag_ids(&again).contains(&"scope-changed"), "{again:?}");
}

#[test]
fn scope_changed_cannot_be_waived() {
    let repo = repo_on_work(&[("src/lib.rs", LIB)]);
    let sha = repo.commit(&[("src/b.rs", "x\n")], "add b", start_secs() + 10);
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    fx.write_report("api", &full_report(&[&sha]));
    assert_eq!(run_one(&mut fx, "api").verdict, Verdict::Pass);
    let waived = waive(
        &mut fx.ctx(),
        &role("api"),
        &FlagId::new("scope-changed"),
        "ok",
    );
    assert!(
        matches!(waived, Err(VerifyError::UnknownFlag(_))),
        "{waived:?}"
    );
}
