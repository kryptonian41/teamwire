//! `--base <ref>`: report protected files changed and tests weakened since
//! the merge-base of `<ref>` and `HEAD`.
//!
//! Never: fail the command for what it reports. Some tickets own protected
//! paths, and `#[expect(` is allowed with a reason, so a program cannot tell
//! which lines are wrong. The lead reads the lines at each integration.
//!
//! Invariants: the diff is the working tree against the merge-base, so
//! committed and uncommitted changes are both seen.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::Findings;

const PROTECTED_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "clippy.toml",
    "rustfmt.toml",
    "rust-toolchain.toml",
    ".cargo/config.toml",
];
const PROTECTED_DIRS: &[&str] = &["scripts/", "xtask/", "tests/fakes/", "docs/adr/"];
const TEST_ATTRS: &[&str] = &["#[test]", "#[tokio::test"];
const WEAKENING_ATTRS: &[&str] = &[
    "#[ignore",
    "#[should_panic",
    "#[expect(",
    "#[allow(",
    "#![allow(",
    "#![expect(",
];

pub(crate) fn check_base(root: &Path, base: &str, found: &mut Findings) -> Result<()> {
    let merge_base = git(root, &["merge-base", base, "HEAD"])?.trim().to_owned();
    let names = git(root, &["diff", "--name-only", &merge_base])?;
    for path in names.lines().filter(|p| is_protected(p)) {
        found.notes.push(format!("PROTECTED-CHANGED {path}"));
    }
    let diff = git(root, &["diff", "-U0", &merge_base, "--", "*.rs"])?;
    found.notes.extend(weakened(&diff));
    Ok(())
}

fn is_protected(path: &str) -> bool {
    PROTECTED_FILES.contains(&path) || PROTECTED_DIRS.iter().any(|d| path.starts_with(d))
}

#[expect(
    clippy::disallowed_methods,
    reason = "xtask is a developer tool that runs git directly"
)]
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `WEAKENED <path>:<line>` for each net-removed test and each added
/// `#[ignore`, `#[should_panic`, `#[expect(` or `#[allow(`, from a
/// zero-context unified diff. A test moved inside its file is not a removal.
fn weakened(diff: &str) -> Vec<String> {
    let mut scan = Scan::default();
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            scan.finish_file();
        } else if scan.in_hunk && !line.starts_with("@@") {
            scan.hunk_line(line);
        } else if let Some(path) = line.strip_prefix("--- ") {
            path.strip_prefix("a/")
                .unwrap_or("")
                .clone_into(&mut scan.old_file);
        } else if let Some(path) = line.strip_prefix("+++ ") {
            path.strip_prefix("b/")
                .unwrap_or("")
                .clone_into(&mut scan.file);
        } else if let Some(hunk) = line.strip_prefix("@@ -") {
            (scan.old_line, scan.new_line) = hunk_starts(hunk);
            scan.in_hunk = true;
        }
    }
    scan.finish_file();
    scan.out
}

#[derive(Default)]
struct Scan {
    out: Vec<String>,
    file: String,
    old_file: String,
    in_hunk: bool,
    old_line: usize,
    new_line: usize,
    removed_tests: Vec<usize>,
    added_tests: usize,
}

impl Scan {
    fn name(&self) -> String {
        file_or(&self.file, &self.old_file)
    }

    fn hunk_line(&mut self, line: &str) {
        if let Some(text) = line.strip_prefix('-') {
            if starts_with_any(text, TEST_ATTRS) {
                self.removed_tests.push(self.old_line);
            }
            self.old_line += 1;
        } else if let Some(text) = line.strip_prefix('+') {
            if starts_with_any(text, TEST_ATTRS) {
                self.added_tests += 1;
            }
            if starts_with_any(text, WEAKENING_ATTRS) {
                let entry = format!("WEAKENED {}:{}", self.name(), self.new_line);
                self.out.push(entry);
            }
            self.new_line += 1;
        }
    }

    /// Report the tests this file lost on balance, then reset for the next.
    fn finish_file(&mut self) {
        let net = self.removed_tests.len().saturating_sub(self.added_tests);
        let name = self.name();
        for line in self.removed_tests.iter().take(net) {
            self.out.push(format!("WEAKENED {name}:{line}"));
        }
        self.removed_tests.clear();
        self.added_tests = 0;
        self.file.clear();
        self.old_file.clear();
        self.in_hunk = false;
    }
}

/// A deleted file has `+++ /dev/null`, so its old path names it.
fn file_or(file: &str, old_file: &str) -> String {
    if file.is_empty() { old_file } else { file }.to_owned()
}

fn starts_with_any(text: &str, prefixes: &[&str]) -> bool {
    let text = text.trim_start();
    prefixes.iter().any(|p| text.starts_with(p))
}

/// The old and new start lines of `-a,b +c,d @@ ...` (`,b` may be absent).
fn hunk_starts(hunk: &str) -> (usize, usize) {
    let number = |s: &str| {
        s.trim_start_matches(['-', '+'])
            .split(',')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    let mut parts = hunk.split_whitespace();
    let old = parts.next().map_or(0, number);
    let new = parts.next().map_or(0, number);
    (old, new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{clean_workspace, empty_metadata, write};

    fn git_ok(root: &Path, args: &[&str]) {
        git(root, args).unwrap();
    }

    fn committed_workspace() -> tempfile::TempDir {
        let dir = clean_workspace();
        let root = dir.path();
        git_ok(root, &["init", "-q", "-b", "main"]);
        git_ok(root, &["config", "user.email", "t@example.com"]);
        git_ok(root, &["config", "user.name", "t"]);
        write(
            root,
            "crates/teamwire-a/src/lib.rs",
            "//! Purpose.\n#[test]\nfn one() {}\n#[test]\nfn two() {}\n",
        );
        git_ok(root, &["add", "."]);
        git_ok(root, &["commit", "-q", "-m", "base"]);
        git_ok(root, &["checkout", "-q", "-b", "work"]);
        dir
    }

    fn report(root: &Path) -> Vec<String> {
        let mut found = crate::tidy(root, &empty_metadata()).unwrap();
        check_base(root, "main", &mut found).unwrap();
        assert!(found.problems.is_empty(), "{found:?}");
        found.notes
    }

    #[test]
    fn untouched_branch_reports_nothing() {
        let dir = committed_workspace();
        assert_eq!(report(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn changed_protected_files_are_reported_but_do_not_fail() {
        let dir = committed_workspace();
        write(dir.path(), "clippy.toml", "x = 1\n");
        write(dir.path(), "tests/fakes/src/lib.rs", "//! Fakes.\n");
        write(dir.path(), "docs/adr/0100-x.md", "x\n");
        write(dir.path(), "docs/other.md", "not protected\n");
        git_ok(dir.path(), &["add", "."]);
        git_ok(dir.path(), &["commit", "-q", "-m", "protected"]);
        assert_eq!(
            report(dir.path()),
            [
                "PROTECTED-CHANGED clippy.toml",
                "PROTECTED-CHANGED docs/adr/0100-x.md",
                "PROTECTED-CHANGED tests/fakes/src/lib.rs",
            ]
        );
    }

    #[test]
    fn removed_test_is_reported_with_its_old_line() {
        let dir = committed_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/src/lib.rs",
            "//! Purpose.\n#[test]\nfn one() {}\n",
        );
        assert_eq!(
            report(dir.path()),
            ["WEAKENED crates/teamwire-a/src/lib.rs:4"]
        );
    }

    #[test]
    fn added_ignore_should_panic_expect_and_allow_are_reported() {
        let dir = committed_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/src/lib.rs",
            "//! Purpose.\n#[test]\n#[ignore]\nfn one() {}\n#[test]\n#[should_panic]\nfn two() {}\n#[expect(unused, reason = \"x\")]\nfn three() {}\n#[allow(unused)]\nfn four() {}\n",
        );
        assert_eq!(
            report(dir.path()),
            [
                "WEAKENED crates/teamwire-a/src/lib.rs:3",
                "WEAKENED crates/teamwire-a/src/lib.rs:6",
                "WEAKENED crates/teamwire-a/src/lib.rs:8",
                "WEAKENED crates/teamwire-a/src/lib.rs:10",
            ]
        );
    }

    #[test]
    fn moving_a_test_within_its_file_is_not_a_removal() {
        let dir = committed_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/src/lib.rs",
            "//! Purpose.\n#[test]\nfn two() {}\n#[test]\nfn one() {}\n",
        );
        assert_eq!(report(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn deleting_a_file_of_tests_is_reported() {
        let dir = committed_workspace();
        std::fs::remove_file(dir.path().join("crates/teamwire-a/src/lib.rs")).unwrap();
        let lines = report(dir.path());
        assert_eq!(
            lines,
            [
                "WEAKENED crates/teamwire-a/src/lib.rs:2",
                "WEAKENED crates/teamwire-a/src/lib.rs:4",
            ]
        );
    }

    #[test]
    fn unknown_base_is_an_error() {
        let dir = committed_workspace();
        let mut found = Findings::default();
        assert!(check_base(dir.path(), "no-such-ref", &mut found).is_err());
    }
}
