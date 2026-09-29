//! The report check: what counts as a gap in a member's report file
//! (REP-5, REP-6, INV-18, INV-19). Every test of what counts lives here.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/fixture.rs"]
pub mod fixture;

use fixture::{Fixture, Repo, implementer, start_secs};
use teamwire_core::{MemberKind, Mode, ReportGaps, RunOutput};
use teamwire_verify::{VerifyError, report_check};

fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn gaps(fx: &mut Fixture, role: &str) -> ReportGaps {
    let member = fx.member(role);
    report_check(&fx.ctx(), &member).unwrap()
}

/// `main` has one commit long before the member started; the member works on `work`.
fn repo() -> Repo {
    let repo = Repo::init(&[("README.md", "hi\n")], start_secs() - 10_000);
    repo.git(&["checkout", "--quiet", "-b", "work"]);
    repo
}

fn member_of(repo: &Repo) -> Fixture {
    let mut fx = Fixture::new();
    fx.add_member(&implementer("api", repo, Some("true"), &["src/"]));
    fx
}

const DECISIONS: &str = "\nDecisions I made: none\n";

#[test]
fn a_missing_report_file_is_the_only_gap_reported() {
    let repo = repo();
    repo.commit(&[("src/a.rs", "")], "own", start_secs() + 5);
    let mut fx = member_of(&repo);
    assert_eq!(
        gaps(&mut fx, "api"),
        ReportGaps {
            report_missing: true,
            commits_without_line: Vec::new(),
            decisions_missing: false,
        }
    );
}

#[test]
fn a_complete_report_has_no_gaps() {
    let repo = repo();
    let sha = repo.commit(&[("src/a.rs", "")], "own", start_secs() + 5);
    let mut fx = member_of(&repo);
    fx.write_report("api", &format!("- {} wrote a\n{DECISIONS}", short(&sha)));
    assert!(gaps(&mut fx, "api").is_empty());
}

#[test]
fn a_commit_without_a_line_is_named_by_its_seven_characters_oldest_first() {
    let repo = repo();
    let first = repo.commit(&[("src/a.rs", "1")], "first", start_secs() + 5);
    let second = repo.commit(&[("src/a.rs", "2")], "second", start_secs() + 6);
    let third = repo.commit(&[("src/a.rs", "3")], "third", start_secs() + 7);
    let mut fx = member_of(&repo);
    fx.write_report("api", &format!("- {} wrote a\n{DECISIONS}", short(&second)));
    let found = gaps(&mut fx, "api");
    assert_eq!(found.commits_without_line, [short(&first), short(&third)]);
    assert!(!found.report_missing && !found.decisions_missing);
}

#[test]
fn a_report_without_the_decisions_list_is_a_gap_in_any_letter_case() {
    let repo = repo();
    let sha = repo.commit(&[("src/a.rs", "")], "own", start_secs() + 5);
    let mut fx = member_of(&repo);
    fx.write_report("api", &format!("- {} wrote a\n", short(&sha)));
    let found = gaps(&mut fx, "api");
    assert!(found.decisions_missing);
    assert!(found.commits_without_line.is_empty());
    fx.write_report(
        "api",
        &format!("- {} wrote a\n\nDECISIONS I MADE\n- none\n", short(&sha)),
    );
    assert!(gaps(&mut fx, "api").is_empty());
}

#[test]
fn merged_amendment_needs_no_lines() {
    let repo = repo();
    let own = repo.commit(&[("src/a.rs", "")], "own work", start_secs() + 10);
    // The lead amends the contract on `main`, after the member started, and
    // the member merges it. Author time alone would not exclude these commits.
    repo.git(&["checkout", "--quiet", "main"]);
    repo.commit(
        &[("contract.md", "v2\n")],
        "amend the contract",
        start_secs() + 20,
    );
    repo.commit(
        &[("contract.md", "v3\n")],
        "amend it again",
        start_secs() + 21,
    );
    repo.git(&["checkout", "--quiet", "work"]);
    repo.git_at(
        &["merge", "--no-ff", "--quiet", "-m", "merge main", "main"],
        start_secs() + 30,
    );
    let mut fx = Fixture::new();
    // Started from the very first commit, as launch records it.
    fx.add_member(&implementer("api", &repo, Some("true"), &["src/"]));
    fx.write_report("api", &format!("- {} wrote a\n{DECISIONS}", short(&own)));
    assert!(gaps(&mut fx, "api").is_empty());

    // Without its own line, the member's commit is still missing, so the
    // check is not simply switched off by the merge.
    fx.write_report("api", DECISIONS);
    assert_eq!(gaps(&mut fx, "api").commits_without_line, [short(&own)]);
}

#[test]
fn rebased_stack_counts_own_commits_only() {
    // Someone else's stack, authored long before the member started, ends up
    // between the recorded start and HEAD after a rebase. They are first-parent
    // and not merges; only their author time tells them apart.
    let repo = repo();
    repo.commit(&[("stack/one.rs", "")], "stack one", start_secs() - 5_000);
    repo.commit(&[("stack/two.rs", "")], "stack two", start_secs() - 4_000);
    let own = repo.commit(&[("src/a.rs", "")], "own", start_secs() + 10);
    let mut fx = member_of(&repo);
    fx.write_report("api", &format!("- {} wrote a\n{DECISIONS}", short(&own)));
    assert!(gaps(&mut fx, "api").is_empty());
}

#[test]
fn a_commit_authored_up_to_120_seconds_before_the_start_still_counts() {
    let repo = repo();
    let inside = repo.commit(&[("src/a.rs", "1")], "just before", start_secs() - 120);
    repo.commit(&[("src/a.rs", "2")], "too early", start_secs() - 121);
    let mut fx = member_of(&repo);
    fx.write_report("api", DECISIONS);
    // `git log` lists them oldest first by history, so the order of the two
    // commits above is: "just before", then "too early".
    assert_eq!(gaps(&mut fx, "api").commits_without_line, [short(&inside)]);
}

#[test]
fn only_implementers_in_a_worktree_owe_a_line_per_commit() {
    let repo = repo();
    repo.commit(&[("src/a.rs", "")], "own", start_secs() + 5);
    for (kind, mode) in [
        (MemberKind::Reviewer, Mode::Worktree),
        (MemberKind::Implementer, Mode::Pane),
    ] {
        let mut fx = Fixture::new();
        let mut setup = implementer("api", &repo, Some("true"), &["src/"]);
        setup.kind = kind;
        setup.mode = mode;
        fx.add_member(&setup);
        fx.write_report("api", "no list here\n");
        let found = gaps(&mut fx, "api");
        assert!(found.commits_without_line.is_empty(), "{kind:?} {mode:?}");
        assert!(found.decisions_missing, "{kind:?} {mode:?}");
    }
}

#[test]
fn a_git_failure_is_an_error_the_caller_turns_into_no_gaps() {
    let repo = repo();
    let mut fx = Fixture::new();
    let mut setup = implementer("api", &repo, Some("true"), &["src/"]);
    setup.base_sha = Some("0000000000000000000000000000000000000000".to_owned());
    fx.add_member(&setup);
    fx.write_report("api", DECISIONS);
    let member = fx.member("api");
    let err = report_check(&fx.ctx(), &member).unwrap_err();
    assert!(matches!(err, VerifyError::Git(_)), "{err:?}");
}

#[test]
fn every_git_call_of_the_check_has_a_ten_second_bound_and_a_hang_is_an_error() {
    let repo = repo();
    repo.commit(&[("src/a.rs", "")], "own", start_secs() + 5);
    let mut fx = member_of(&repo);
    fx.write_report("api", DECISIONS);
    // A git that hangs: the platform reports the timeout, as `run` does.
    fx.platform.script_next(
        "git",
        RunOutput {
            exit_code: RunOutput::TIMEOUT_EXIT,
            stdout: Vec::new(),
            stderr: Vec::new(),
            timed_out: true,
        },
    );
    let member = fx.member("api");
    let err = report_check(&fx.ctx(), &member).unwrap_err();
    assert!(matches!(err, VerifyError::Git(_)), "{err:?}");
    let commands = fx.platform.commands();
    assert!(!commands.is_empty());
    for (cmd, timeout) in commands {
        assert_eq!(cmd.program, "git");
        assert_eq!(timeout, std::time::Duration::from_secs(10), "{cmd:?}");
    }
}

#[test]
fn the_report_path_helper_names_the_file_the_check_reads() {
    let repo = repo();
    let fx = member_of(&repo);
    fx.write_report("api", "x");
    let path = teamwire_verify::report_path(&fx.team_dir(), &fx.member("api").role);
    assert_eq!(path, fx.team_dir().join("reports").join("api.md"));
    assert!(path.is_file());
}
