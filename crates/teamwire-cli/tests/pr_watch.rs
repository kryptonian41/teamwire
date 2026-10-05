//! `teamwire pr-watch`, run as the real binary with a fake `gh` script first
//! on `PATH`. The real `gh` is never called.
//!
//! The fake reads `$FAKE_GH_DIR`. Its answer to the nth call of
//! `gh pr checks N` (or `gh pr view N`) is the file `checks-N-<n>.json` (or
//! `view-N-<n>.json`); a call past the last file gets the last file. It logs
//! every call's arguments to `calls.log` and always exits 8, as `gh pr
//! checks` does while checks are pending: the watch must ignore that.
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
n=$(cat "$d/n-$kind-$pr" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$d/n-$kind-$pr"
while [ "$n" -gt 0 ] && [ ! -f "$d/$kind-$pr-$n.json" ]; do
  n=$((n - 1))
done
[ "$n" -gt 0 ] && cat "$d/$kind-$pr-$n.json"
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
        Self { dir }
    }

    fn answer(&self, kind: &str, pr: u64, n: u32, body: &str) {
        std::fs::write(self.dir.path().join(format!("{kind}-{pr}-{n}.json")), body).unwrap();
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
    format!(r#"{{"state":"{state}","mergeable":"{mergeable}","mergeStateStatus":"{merge_state}"}}"#)
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
            "pr view 7 -R o/r --json mergeable,mergeStateStatus,state"
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
fn a_call_that_prints_no_json_is_skipped_and_tried_again() {
    let bed = Bed::new();
    bed.answer("checks", 7, 1, "gh: HTTP 502");
    bed.answer("view", 7, 1, "");
    bed.answer("checks", 7, 2, &checks(&[("a", "pass")]));
    let run = bed.watch(&["7"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.lines,
        [
            "WATCHING o/r#7 (poll 1s)",
            "o/r#7 DONE: 1 passed, 0 failed, 0 skipped"
        ]
    );
    assert!(bed.calls().len() >= 4, "{:?}", bed.calls());
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
