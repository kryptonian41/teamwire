//! The rules of ADR 0049 for `teamwire speckit draft`, on a fixture
//! repository: a line list and a possessive after a path, a partial path
//! that a new file could also be, a module named only by imports, and a
//! root file named by a relative link or by a bare task word.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and makes a throwaway git repository"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::Path;

/// Each task names the words of one rule. T002 and T007 both name
/// `tests/preset.rs`, so a word the overlap counts pairs Phase 2 and
/// Phase 3.
const TASKS: &str = "# Tasks: rules

## Phase 1: Setup

- [ ] T001 Read `AGENTS.md` and `CLAUDE.md`.

## Phase 2: User Story 1 - first (Priority: P1)

- [ ] T002 [US1] Add a test to `tests/preset.rs`.
- [ ] T003 [US1] Fix `crates/m/tests/send.rs:718,852`.
- [ ] T004 [US1] Follow contracts/amend-cli.md's example.
- [ ] T005 [US1] Change `apps/ai/shared/db/enums.py` and `apps/ai/gateway/agent_sessions/schemas/agent_sessions.py`.

## Phase 3: User Story 2 - second (Priority: P2)

- [ ] T006 [US2] Write `tests/x.rs` and `tests/brand_new.rs`.
- [ ] T007 [US2] Edit `agents/x/prompts/system.md` and `tests/preset.rs`.

## Phase 4: Polish

- [ ] T008 Update `AGENTS.md` and `sonar-project.properties:73`.
";

const RULES_CI: &str = "name: rules
on:
  push:
    paths: [\"AGENTS.md\"]
jobs:
  run:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The root folder `tests/` holds a tracked file, so the new-file rule
/// accepts `tests/preset.rs`, while `crates/c/tests/preset.rs` also ends in
/// it. Each file under `apps/` or at the root that holds `AGENTS.md` shows
/// one way of naming a root file.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let files: [(&str, &str); 30] = [
        ("specs/001-x/tasks.md", TASKS),
        ("specs/001-x/contracts/amend-cli.md", "# amend\n"),
        ("crates/m/tests/send.rs", "// send\n"),
        ("tests/fakes/a.rs", "// fake\n"),
        ("crates/c/tests/preset.rs", "// preset\n"),
        ("crates/a/tests/x.rs", "// a\n"),
        ("crates/b/tests/x.rs", "// b\n"),
        ("crates/d/tests/x.rs", "// d\n"),
        ("apps/ai/shared/db/enums.py", "class E: pass\n"),
        (
            "apps/ai/gateway/agent_sessions/schemas/agent_sessions.py",
            "class S: pass\n",
        ),
        (
            "apps/ai/.importlinter",
            "[contract]\nlayers =\n    gateway.agent_sessions.schemas.agent_sessions\n",
        ),
        (
            "apps/ai/x/only_import.py",
            "from shared.db.enums import ReportVisibility\n",
        ),
        ("apps/ai/x/import_as.py", "import shared.db.enums as e\n"),
        ("apps/ai/x/import_list.py", "import os, shared.db.enums\n"),
        (
            "apps/ai/x/commented.py",
            "from shared.db.enums import ReportVisibility\n\n# see shared.db.enums for the values\n",
        ),
        ("agents/x/prompts/system.md", "You are a deck editor.\n"),
        ("apps/ai/live_turn.py", "load(prompts.system)\n"),
        ("AGENTS.md", "rules\n"),
        ("CLAUDE.md", "@AGENTS.md\n"),
        ("sonar-project.properties", "sonar.projectKey=x\n"),
        ("apps/ai/CLAUDE.md", "Read ../../AGENTS.md first.\n"),
        ("apps/web/CLAUDE.md", "Read ../AGENTS.md first.\n"),
        ("GUIDE.md", "See ./AGENTS.md.\n"),
        ("apps/x/CLAUDE.md", "See ./AGENTS.md and @AGENTS.md.\n"),
        ("docs/x.md", "See apps/ai/AGENTS.md.\n"),
        ("CONTRIBUTING.md", "see AGENTS.md\n"),
        ("apps/ai/AGENTS.md", "nested rules\n"),
        ("apps/x/deep/far.md", "Read ../../../AGENTS.md first.\n"),
        ("apps/x/deep/over.md", "Read ../../../../AGENTS.md first.\n"),
        (".github/workflows/rules.yml", RULES_CI),
    ];
    for (path, text) in files {
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "fixture"]);
    dir
}

fn draft(repo: &Path) -> String {
    let home = tempfile::tempdir().unwrap();
    let tasks = repo.join("specs/001-x/tasks.md");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(["speckit", "draft", tasks.to_str().unwrap()])
        .env_remove("HERDR_ENV")
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

/// The indented lines after the line `head`.
fn block<'a>(out: &'a str, head: &str) -> Vec<&'a str> {
    out.lines()
        .skip_while(|l| *l != head)
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .collect()
}

fn line<'a>(out: &'a str, start: &str) -> &'a str {
    out.lines()
        .find(|l| l.starts_with(start))
        .unwrap_or_default()
}

/// The line of the DRAFT list `name` whose first word is `path`.
fn entry<'a>(out: &'a str, name: &str, path: &str) -> Option<&'a str> {
    block(out, &format!("{name}:"))
        .into_iter()
        .find(|l| l.split_whitespace().next() == Some(path))
}

/// Every line of every DRAFT list.
fn drafts(out: &str) -> Vec<&str> {
    out.lines()
        .skip_while(|l| !l.starts_with("DRAFT owns lists."))
        .collect()
}

/// Rule E: a `:718,852` line list and a trailing `'s` come off before the
/// code rule, so T003 and T004 each name their file.
#[test]
fn a_line_list_and_a_possessive_leave_the_path() {
    let repo = repo();
    let out = draft(repo.path());
    assert!(
        entry(&out, "US1", "crates/m/tests/send.rs").is_some_and(|l| l.ends_with("(task path)")),
        "{out}"
    );
    assert!(
        entry(&out, "US1", "specs/001-x/contracts/amend-cli.md")
            .is_some_and(|l| l.ends_with("(task path)")),
        "{out}"
    );
    let no_file = line(&out, "Tasks that change no file:");
    assert!(
        !no_file.contains("T003") && !no_file.contains("T004"),
        "{out}"
    );
    assert!(!out.contains(":718,852") && !out.contains("md's"), "{out}");
}

/// Rule D: `tests/preset.rs` could be a new file in the tracked root folder
/// `tests/`, or the tracked `crates/c/tests/preset.rs`. It has two readings,
/// so it is no path: it is printed with the tracked path it also ends, it
/// goes in no draft, and it still counts in the overlap. `tests/x.rs` ends
/// three tracked paths. `tests/brand_new.rs` ends none, so it stays a new
/// file.
#[test]
fn a_partial_path_with_two_readings_is_no_path() {
    let repo = repo();
    let out = draft(repo.path());
    let names = block(&out, "Names that are not repository paths:");
    assert!(
        names.contains(&"  T002: tests/preset.rs (also ends crates/c/tests/preset.rs)"),
        "{out}"
    );
    assert!(
        names.contains(&"  T006: tests/x.rs (also ends crates/a/tests/x.rs and 2 more)"),
        "{out}"
    );
    for word in ["tests/preset.rs", "tests/x.rs", "crates/c/tests/preset.rs"] {
        assert!(
            !drafts(&out)
                .iter()
                .any(|l| l.split_whitespace().next() == Some(word)),
            "{word}: {out}"
        );
    }
    assert!(
        entry(&out, "US2", "tests/brand_new.rs").is_some_and(|l| l.ends_with("(task path)")),
        "{out}"
    );
    let shared = block(&out, "Shared files between phases:");
    assert!(
        shared
            .iter()
            .any(|l| l.starts_with("  Phase 2 and Phase 3:") && l.contains("tests/preset.rs")),
        "{out}"
    );
}
