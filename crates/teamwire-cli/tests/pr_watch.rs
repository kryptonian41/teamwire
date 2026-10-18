//! `teamwire pr-watch`, run as the real binary with a fake `gh` script first
//! on `PATH`. The real `gh` is never called.
//!
//! The fake reads `$FAKE_GH_DIR`. Its answer to the nth call of
//! `gh pr checks N` (or `gh pr view N`) is the file `checks-N-<n>.json` (or
//! `view-N-<n>.json`); a call past the last file gets the last file. It logs
//! every call's arguments to `calls.log` and always exits 8, as `gh pr
//! checks` does while checks are pending: the watch must ignore that. For
//! `gh api <path>` the kind is `api` and the pull-request slot is the path
//! with `/` turned into `_`; a call with no answer file exits 1 and one with
//! an answer file exits 0, because the watch trusts `api` exit codes. A file
//! `checks-N-<n>.err` is printed on stderr beside the answer.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

const FAKE_GH: &str = r#"#!/bin/sh
d="$FAKE_GH_DIR"
echo "$*" >> "$d/calls.log"
kind="$2"
pr="$3"
if [ "$1" = api ]; then
  kind=api
  for last; do :; done
  pr=$(echo "$last" | tr / _)
fi
n=$(cat "$d/n-$kind-$pr" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$d/n-$kind-$pr"
while [ "$n" -gt 0 ] && [ ! -f "$d/$kind-$pr-$n.json" ]; do
  n=$((n - 1))
done
[ "$n" -gt 0 ] && cat "$d/$kind-$pr-$n.json"
[ "$n" -gt 0 ] && [ -f "$d/$kind-$pr-$n.err" ] && cat "$d/$kind-$pr-$n.err" >&2
if [ "$1" = api ]; then
  [ "$n" -gt 0 ] && [ -f "$d/$kind-$pr-$n.exit" ] && exit "$(cat "$d/$kind-$pr-$n.exit")"
  [ "$n" -gt 0 ] && exit 0
  exit 1
fi
exit 8
"#;

struct Bed {
    dir: tempfile::TempDir,
}

struct Run {
    code: i32,
    lines: Vec<String>,
}

impl Bed {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let gh = dir.path().join("gh");
        std::fs::write(&gh, FAKE_GH).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bed = Self { dir };
        // By default the base branch `main` requires nothing.
        bed.api("repos/o/r/branches/main", "{}");
        bed.api("repos/o/r/rules/branches/main", "[]");
        bed
    }

    fn answer(&self, kind: &str, pr: u64, n: u32, body: &str) {
        std::fs::write(self.dir.path().join(format!("{kind}-{pr}-{n}.json")), body).unwrap();
    }

    /// The answer to `gh api <path>`; a path with no answer makes `gh` fail.
    fn api(&self, path: &str, body: &str) {
        let slot = path.replace('/', "_");
        std::fs::write(self.dir.path().join(format!("api-{slot}-1.json")), body).unwrap();
    }

    /// Makes `gh api <path>` exit `code` after printing its answer, and print
    /// `stderr` on stderr, as `gh` does for an HTTP error.
    fn api_fails(&self, path: &str, code: i32, stderr: &str) {
        let slot = path.replace('/', "_");
        // The fake answers only where an answer file exists; this one is empty.
        std::fs::write(self.dir.path().join(format!("api-{slot}-1.json")), "").unwrap();
        std::fs::write(
            self.dir.path().join(format!("api-{slot}-1.exit")),
            code.to_string(),
        )
        .unwrap();
        std::fs::write(self.dir.path().join(format!("api-{slot}-1.err")), stderr).unwrap();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Runs `pr-watch -R o/r -i 1 <prs>`, killing it after 30 seconds so a
    /// watch that never ends fails the test instead of hanging it.
    fn watch(&self, prs: &[&str]) -> Run {
        let home = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .arg("--home")
            .arg(home.path())
            .args(["pr-watch", "-R", "o/r", "-i", "1"])
            .args(prs)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.dir.path().display()),
            )
            .env("FAKE_GH_DIR", self.dir.path())
            .env_remove("HERDR_ENV")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                unreachable!("pr-watch did not finish in 30 seconds");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().unwrap();
        Run {
            code: out.status.code().unwrap(),
            lines: String::from_utf8(out.stdout)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect(),
        }
    }
}

fn checks(items: &[(&str, &str)]) -> String {
    let items: Vec<String> = items
        .iter()
        .map(|(name, bucket)| format!(r#"{{"name":"{name}","bucket":"{bucket}"}}"#))
        .collect();
    format!("[{}]", items.join(","))
}

fn view(state: &str, mergeable: &str, merge_state: &str) -> String {
    format!(
        r#"{{"state":"{state}","mergeable":"{mergeable}","mergeStateStatus":"{merge_state}","baseRefName":"main"}}"#
    )
}

#[test]
fn all_checks_passing_prints_one_done_line_and_exits_zero() {
    let bed = Bed::new();
    bed.answer(
        "checks",
        7,
        1,
        &checks(&[("build", "pass"), ("lint", "pass")]),
    );
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 2 passed, 0 failed, 0 skipped"
        ]
    );
    // The repository goes to `gh` with -R, and nothing else is asked.
    assert_eq!(
        bed.calls(),
        [
            "pr checks 7 -R o/r --json name,bucket",
            "pr view 7 -R o/r --json mergeable,mergeStateStatus,state,baseRefName",
            "api repos/o/r/branches/main",
            "api --paginate --slurp repos/o/r/rules/branches/main"
        ]
    );
}

#[test]
fn a_failure_is_said_once_and_the_run_exits_one() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer(
        "checks",
        7,
        1,
        &checks(&[("lint", "fail"), ("build", "pending")]),
    );
    bed.answer(
        "checks",
        7,
        2,
        &checks(&[("lint", "fail"), ("build", "pending")]),
    );
    bed.answer(
        "checks",
        7,
        3,
        &checks(&[("lint", "cancel"), ("build", "pass")]),
    );
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 1, "{:?}", run.lines);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 FAIL: lint",
            "o/r#7 DONE: 1 passed, 1 failed, 0 skipped [lint]"
        ]
    );
}

#[test]
fn skipped_checks_are_counted_and_pending_ones_keep_the_watch_going() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("checks", 7, 1, &checks(&[("a", "pending"), ("b", "pass")]));
    bed.answer(
        "checks",
        7,
        2,
        &checks(&[("a", "pass"), ("b", "pass"), ("c", "skipping")]),
    );
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 2 passed, 0 failed, 1 skipped"
        ]
    );
}

#[test]
fn a_change_of_mergeability_is_said_but_the_first_clean_verdict_is_not() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("view", 7, 2, &view("OPEN", "UNKNOWN", "UNKNOWN"));
    bed.answer("view", 7, 3, &view("OPEN", "CONFLICTING", "DIRTY"));
    bed.answer("checks", 7, 1, &checks(&[("a", "pending")]));
    bed.answer("checks", 7, 2, &checks(&[("a", "pending")]));
    bed.answer("checks", 7, 3, &checks(&[("a", "pending")]));
    bed.answer("checks", 7, 4, &checks(&[("a", "pass")]));
    let run = bed.watch(&["7"]);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 MERGEABLE: CONFLICTING/DIRTY",
            "o/r#7 DONE: 1 passed, 0 failed, 0 skipped"
        ]
    );
}

#[test]
fn a_conflict_on_the_first_look_is_said_at_once() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, &view("OPEN", "CONFLICTING", "DIRTY"));
    bed.answer("checks", 7, 1, &checks(&[("a", "pass")]));
    let run = bed.watch(&["7"]);
    assert_eq!(
        run.lines.get(1).map(String::as_str),
        Some("o/r#7 MERGEABLE: CONFLICTING/DIRTY | o/r#7 DONE: 1 passed, 0 failed, 0 skipped")
    );
}

#[test]
fn a_blocked_or_draft_pull_request_is_said_once_and_the_done_line_names_the_state() {
    for state in ["BLOCKED", "DRAFT"] {
        let bed = Bed::new();
        bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", state));
        bed.answer("view", 7, 2, &view("OPEN", "MERGEABLE", state));
        bed.answer("checks", 7, 1, &checks(&[("build", "pending")]));
        bed.answer("checks", 7, 2, &checks(&[("build", "pass")]));
        let run = bed.watch(&["7"]);
        assert_eq!(
            run.lines.get(1..),
            Some(
                [
                    format!("o/r#7 MERGEABLE: MERGEABLE/{state}"),
                    format!("o/r#7 DONE: 1 passed, 0 failed, 0 skipped (merge state: {state})"),
                ]
                .as_slice()
            )
        );
    }
}

#[test]
fn a_call_that_prints_no_json_is_skipped_and_tried_again() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, "gh: HTTP 502");
    bed.answer("view", 7, 1, "");
    bed.answer("checks", 7, 2, &checks(&[("a", "pass")]));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    // The view never answered, so the base branch and its required checks
    // stay unknown.
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 1 passed, 0 failed, 0 skipped (required checks unknown)"
        ]
    );
    assert!(bed.calls().len() >= 4, "{:?}", bed.calls());
}

#[test]
fn an_outage_of_gh_is_said_once_per_run_of_three_failed_polls() {
    let bed = Bed::new();
    // Polls 1-5 fail (the line comes at the third, not again at the fourth
    // or fifth), poll 6 answers, polls 7-9 fail again, poll 10 finishes.
    for n in [1, 2, 3, 4, 5, 7, 8, 9] {
        bed.answer("checks", 7, n, "gh: HTTP 502");
        bed.answer("view", 7, n, "");
    }
    bed.answer("checks", 7, 6, &checks(&[("a", "pending")]));
    bed.answer("view", 7, 6, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("checks", 7, 10, &checks(&[("a", "pass")]));
    bed.answer("view", 7, 10, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    let said = "o/r#7 UNREADABLE: gh gave no answer for 3 polls";
    let count = run.lines.iter().filter(|l| l.as_str() == said).count();
    assert_eq!(count, 2, "{:?}", run.lines);
}

#[test]
fn a_merged_pull_request_is_done_without_checks() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, &view("MERGED", "UNKNOWN", "UNKNOWN"));
    bed.answer("checks", 7, 1, &checks(&[("a", "fail")]));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(run.lines, ["WATCHING o/r#7 (poll 1s)", "o/r#7 MERGED"]);
}

#[test]
fn several_pull_requests_share_one_line_per_poll_and_a_done_one_is_not_asked_again() {
    let bed = Bed::new();
    bed.answer("view", 1, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("view", 2, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("checks", 1, 1, &checks(&[("a", "pass")]));
    bed.answer("checks", 2, 1, &checks(&[("a", "pending")]));
    bed.answer("checks", 2, 2, &checks(&[("a", "fail")]));
    let run = bed.watch(&["1", "2"]);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#1, o/r#2 (poll 1s)",
            "o/r#1 DONE: 1 passed, 0 failed, 0 skipped",
            "o/r#2 FAIL: a | o/r#2 DONE: 0 passed, 1 failed, 0 skipped [a]"
        ]
    );
    let asked_about_one = bed.calls().iter().filter(|c| c.contains(" 1 -R")).count();
    assert_eq!(asked_about_one, 2, "one checks call and one view call");
}

#[test]
fn the_default_interval_is_forty_five_seconds() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, &checks(&[("a", "pass")]));
    let home = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(["pr-watch", "-R", "o/r", "7"])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", bed.dir.path().display()),
        )
        .env("FAKE_GH_DIR", bed.dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("WATCHING o/r#7 (poll 45s)\n")
    );
}

#[test]
fn no_checks_is_done() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, "[]");
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 0 passed, 0 failed, 0 skipped"
        ]
    );
}

#[test]
fn gh_saying_no_checks_were_reported_is_done_not_polled_for_ever() {
    let bed = Bed::new();
    // Real `gh pr checks --json` prints nothing on stdout for such a pull request.
    bed.answer("checks", 7, 1, "");
    std::fs::write(
        bed.dir.path().join("checks-7-1.err"),
        "no checks reported on the 'topic' branch\n",
    )
    .unwrap();
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 0 passed, 0 failed, 0 skipped"
        ]
    );
}

/// Right after a push GitHub has no check runs yet. A list that is empty
/// on the first polls and then fills is watched to its end.
#[test]
fn no_checks_on_the_first_polls_is_not_done_before_ci_starts() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, "[]");
    bed.answer("checks", 7, 2, "[]");
    bed.answer("checks", 7, 3, &checks(&[("ci", "pending")]));
    bed.answer("checks", 7, 4, &checks(&[("ci", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 1 passed, 0 failed, 0 skipped"
        ]
    );
}

/// A pull request that never gets checks is done on the third empty poll.
#[test]
fn no_checks_on_three_polls_in_a_row_is_done() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, "[]");
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 0 passed, 0 failed, 0 skipped"
        ]
    );
    let checks_calls = bed
        .calls()
        .iter()
        .filter(|c| c.starts_with("pr checks 7"))
        .count();
    assert_eq!(checks_calls, 3, "{:?}", bed.calls());
}

#[test]
fn a_failure_before_merge_still_exits_1() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, &checks(&[("a", "fail"), ("b", "pending")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("view", 7, 2, &view("MERGED", "UNKNOWN", "UNKNOWN"));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 1, "{:?}", run.lines);
    assert!(
        run.lines.contains(&"o/r#7 FAIL: a".to_owned()),
        "{:?}",
        run.lines
    );
    assert!(
        run.lines.contains(&"o/r#7 MERGED".to_owned()),
        "{:?}",
        run.lines
    );
}

#[test]
fn interval_zero_is_a_usage_error() {
    let bed = Bed::new();
    let home = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(["pr-watch", "-R", "o/r", "-i", "0", "7"])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", bed.dir.path().display()),
        )
        .env("FAKE_GH_DIR", bed.dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(!bed.dir.path().join("calls.log").exists(), "gh was called");
}

#[test]
fn the_required_set_is_read_once_per_pull_request_for_the_whole_watch() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, &checks(&[("a", "pending")]));
    bed.answer("checks", 7, 2, &checks(&[("a", "pending")]));
    bed.answer("checks", 7, 3, &checks(&[("a", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.watch(&["7"]);
    let api_calls = bed.calls().iter().filter(|c| c.starts_with("api ")).count();
    assert_eq!(api_calls, 2, "{:?}", bed.calls());
}

fn done_line(run: &Run) -> &str {
    run.lines.last().unwrap().rsplit(" | ").next().unwrap()
}

#[test]
fn a_required_check_that_never_reported_is_named_and_fails_the_run() {
    let bed = Bed::new();
    bed.api(
        "repos/o/r/branches/main",
        r#"{"protection":{"required_status_checks":{"contexts":["build","build-macos"]}}}"#,
    );
    bed.answer(
        "checks",
        7,
        1,
        &checks(&[("build", "pass"), ("lint", "pass")]),
    );
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "BLOCKED"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        done_line(&run),
        "o/r#7 DONE: 2 passed, 0 failed, 0 skipped (required, never reported: build-macos) (merge state: BLOCKED)"
    );
    assert_eq!(run.code, 1);
}

#[test]
fn every_required_check_present_leaves_the_done_line_and_exit_code_alone() {
    let bed = Bed::new();
    bed.api(
        "repos/o/r/branches/main",
        r#"{"protection":{"required_status_checks":{"contexts":["build"],"checks":[{"context":"lint","app_id":1}]}}}"#,
    );
    bed.answer(
        "checks",
        7,
        1,
        &checks(&[("build", "pass"), ("lint", "pass")]),
    );
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(done_line(&run), "o/r#7 DONE: 2 passed, 0 failed, 0 skipped");
    assert_eq!(run.code, 0);
}

#[test]
fn when_neither_endpoint_answers_the_done_line_says_the_required_set_is_unknown() {
    let bed = Bed::new();
    std::fs::remove_file(bed.dir.path().join("api-repos_o_r_branches_main-1.json")).unwrap();
    std::fs::remove_file(
        bed.dir
            .path()
            .join("api-repos_o_r_rules_branches_main-1.json"),
    )
    .unwrap();
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        done_line(&run),
        "o/r#7 DONE: 1 passed, 0 failed, 0 skipped (required checks unknown)"
    );
    // Unknown is not a failure: nothing is known to be missing.
    assert_eq!(run.code, 0);
}

#[test]
fn a_repository_with_only_rules_names_what_its_rules_require_from_every_page() {
    let bed = Bed::new();
    // No classic protection: the branch endpoint says 404. The rules come as
    // `gh api --paginate --slurp` prints them, one array per page.
    std::fs::remove_file(bed.dir.path().join("api-repos_o_r_branches_main-1.json")).unwrap();
    bed.api_fails("repos/o/r/branches/main", 1, "gh: Not Found (HTTP 404)");
    bed.api(
        "repos/o/r/rules/branches/main",
        r#"[[{"type":"pull_request","parameters":{}}],
            [{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"build"},{"context":"deploy-preview"}]}}]]"#,
    );
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "BLOCKED"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        done_line(&run),
        "o/r#7 DONE: 1 passed, 0 failed, 0 skipped (required, never reported: deploy-preview) (merge state: BLOCKED)"
    );
    assert_eq!(run.code, 1);
}

/// A server without rulesets answers 404 for the rules; that is "no rules",
/// not a reason to doubt the classic protection it did read.
#[test]
fn a_404_from_the_rules_endpoint_means_no_rules() {
    let bed = Bed::new();
    bed.api(
        "repos/o/r/branches/main",
        r#"{"protection":{"required_status_checks":{"contexts":["build","lint"]}}}"#,
    );
    std::fs::remove_file(
        bed.dir
            .path()
            .join("api-repos_o_r_rules_branches_main-1.json"),
    )
    .unwrap();
    bed.api_fails(
        "repos/o/r/rules/branches/main",
        1,
        "gh: Not Found (HTTP 404)",
    );
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "BLOCKED"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        done_line(&run),
        "o/r#7 DONE: 1 passed, 0 failed, 0 skipped (required, never reported: lint) (merge state: BLOCKED)"
    );
}

/// One endpoint failing for another reason may hide required checks, so the
/// classic checks alone must not be taken for the whole set.
#[test]
fn a_rules_endpoint_error_that_is_not_a_404_makes_the_set_unknown() {
    let bed = Bed::new();
    bed.api(
        "repos/o/r/branches/main",
        r#"{"protection":{"required_status_checks":{"contexts":["build","lint"]}}}"#,
    );
    std::fs::remove_file(
        bed.dir
            .path()
            .join("api-repos_o_r_rules_branches_main-1.json"),
    )
    .unwrap();
    bed.api_fails(
        "repos/o/r/rules/branches/main",
        1,
        "gh: API rate limit exceeded (HTTP 403)",
    );
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "BLOCKED"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        done_line(&run),
        "o/r#7 DONE: 1 passed, 0 failed, 0 skipped (required checks unknown) (merge state: BLOCKED)"
    );
    assert_eq!(run.code, 0);
}

/// CLEAN means GitHub found every requirement met, so a missing check beside
/// it is named but does not fail the run.
#[test]
fn a_missing_required_check_beside_a_clean_merge_state_is_named_but_not_counted() {
    let bed = Bed::new();
    bed.api(
        "repos/o/r/branches/main",
        r#"{"protection":{"required_status_checks":{"contexts":["build","lint"]}}}"#,
    );
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        done_line(&run),
        "o/r#7 DONE: 1 passed, 0 failed, 0 skipped (required, never reported: lint)"
    );
    assert_eq!(run.code, 0);
}

/// Names match ignoring case and any `Workflow / ` prefix in front of the job.
#[test]
fn a_check_reported_with_another_case_or_a_prefix_counts_as_reported() {
    let bed = Bed::new();
    bed.api(
        "repos/o/r/branches/main",
        r#"{"protection":{"required_status_checks":{"contexts":["Build","test"]}}}"#,
    );
    bed.answer(
        "checks",
        7,
        1,
        &checks(&[("build", "pass"), ("CI / test", "pass")]),
    );
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(done_line(&run), "o/r#7 DONE: 2 passed, 0 failed, 0 skipped");
    assert_eq!(run.code, 0);
}

/// GitHub recomputes the merge state after the last check ends, so a BLOCKED
/// read in the poll that sees the last check pass may be out of date.
#[test]
fn a_blocked_state_at_done_is_read_again_and_the_fresh_one_is_used() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "BLOCKED"));
    bed.answer("view", 7, 2, &view("OPEN", "MERGEABLE", "CLEAN"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 1 passed, 0 failed, 0 skipped"
        ]
    );
    let views = bed
        .calls()
        .iter()
        .filter(|c| c.starts_with("pr view"))
        .count();
    assert_eq!(views, 2, "{:?}", bed.calls());
}

#[test]
fn a_state_that_is_still_blocked_after_the_second_read_is_reported_as_before() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, &checks(&[("build", "pass")]));
    bed.answer("view", 7, 1, &view("OPEN", "MERGEABLE", "BLOCKED"));
    let run = bed.watch(&["7"]);
    assert_eq!(
        run.lines.get(1),
        Some(
            &"o/r#7 MERGEABLE: MERGEABLE/BLOCKED | o/r#7 DONE: 1 passed, 0 failed, 0 skipped (merge state: BLOCKED)"
                .to_owned()
        )
    );
}

#[test]
fn a_pull_request_that_does_not_exist_is_said_once_and_ends_the_watch_failed() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, "");
    std::fs::write(
        bed.dir.path().join("view-7-1.err"),
        "GraphQL: Could not resolve to a PullRequest with the number of 7. (repository.pullRequest)\n",
    )
    .unwrap();
    bed.answer("view", 8, 1, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("checks", 8, 1, &checks(&[("a", "pass")]));
    let run = bed.watch(&["7", "8"]);
    assert_eq!(run.code, 1, "{:?}", run.lines);
    let said = "o/r#7 NOT-FOUND: GraphQL: Could not resolve to a PullRequest with the number of 7. (repository.pullRequest)";
    let count = run
        .lines
        .iter()
        .flat_map(|l| l.split(" | "))
        .filter(|l| *l == said)
        .count();
    assert_eq!(count, 1, "{:?}", run.lines);
}

#[test]
fn another_gh_failure_keeps_polling() {
    let bed = Bed::new();
    bed.answer("view", 7, 1, "");
    std::fs::write(bed.dir.path().join("view-7-1.err"), "gh: HTTP 401\n").unwrap();
    bed.answer("view", 7, 2, &view("OPEN", "MERGEABLE", "CLEAN"));
    bed.answer("checks", 7, 2, &checks(&[("a", "pass")]));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0, "{:?}", run.lines);
    assert!(
        run.lines.iter().all(|l| !l.contains("NOT-FOUND")),
        "{:?}",
        run.lines
    );
}
