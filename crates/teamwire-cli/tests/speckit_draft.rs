//! `teamwire speckit draft`: reads a spec-kit `tasks.md` in a throwaway git
//! repository and prints the shared files, the tasks that can run apart and
//! the DRAFT `owns` lists (ADR 0044).
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and makes a throwaway git repository"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::Path;

/// A task list in this feature's own line shapes: a path with a `:12-30`
/// suffix, a task whose paths sit on indented lines, a `*` token, a task
/// with no path and a task whose file no other task names.
const TASKS: &str = "# Tasks: x

## Phase 1: Setup

- [ ] T001 Read the plan and the brief.

## Phase 2: Foundational

- [X] T002 [P] Write `docs/notes.md`.

## Phase 3: User Story 1 - first (Priority: P1)

- [ ] T003 [US1] Change `src/a.rs:12-30` and apps/ai/harness/tools.py, see https://example.com/a/b.html.
- [x] T004 [US1] Update these:
  - `src/b.rs`
  - `crates/x/src/cmd/speckit/*.rs`

## Phase 4: User Story 2 - second (Priority: P2)

- [ ] T005 [US2] Change `src/a.rs` again.
- [ ] T006 [P] [US2] Write `apps/copilot-runtime/src/x.ts`.

## Phase 5: Polish

- [ ] T007 Run the check once more.
";

const REPLAY: &str = "name: replay
on:
  push:
    paths:
      - \"apps/ai/harness/**\"
jobs:
  run:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";

/// A workflow the reader refuses: it uses an anchor.
const ODD: &str = "name: odd
on:
  push:
    paths: &watched
      - \"src/**\"
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

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(args: &[&str]) -> Run {
    let home = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
        err: String::from_utf8(out.stderr).unwrap(),
    }
}

/// The fixture repository, committed, with its files tracked.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let files: [(&str, &str); 12] = [
        ("specs/001-x/tasks.md", TASKS),
        ("specs/001-x/plan.md", "Change `src/a.rs` and tools.py.\n"),
        ("src/a.rs", "fn a() {}\n"),
        ("src/b.rs", "fn b() {}\n"),
        ("apps/ai/harness/tools.py", "def tool(): pass\n"),
        (
            "apps/ai/.importlinter",
            "[contract]\nlayers = harness.tools\n",
        ),
        (
            "apps/ai/tests/test_targets.py",
            "TARGETS = [\"tools.py\", \"other.py\"]\n",
        ),
        ("apps/ai/notes.txt", "the tools module is old\n"),
        (".github/workflows/replay.yml", REPLAY),
        (".github/workflows/odd.yml", ODD),
        ("crates/x/src/cmd/speckit/one.rs", "// one\n"),
        ("crates/x/src/cmd/speckit/two.rs", "// two\n"),
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

fn draft(repo: &Path) -> Run {
    let tasks = repo.join("specs/001-x/tasks.md");
    let run = teamwire(&["speckit", "draft", tasks.to_str().unwrap()]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    run
}

/// The lines of the DRAFT list headed `name:`, up to the next list.
fn list<'a>(out: &'a str, name: &str) -> Vec<&'a str> {
    let head = format!("{name}:");
    out.lines()
        .skip_while(|l| *l != head)
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .collect()
}

/// The line of the DRAFT list `name` that names `path`.
fn entry<'a>(out: &'a str, name: &str, path: &str) -> Option<&'a str> {
    list(out, name)
        .into_iter()
        .find(|l| l.split_whitespace().next() == Some(path))
}

#[test]
fn phases_that_name_one_file_are_listed_as_sharing_it() {
    let repo = repo();
    let run = draft(repo.path());
    let shared: Vec<&str> = run
        .out
        .lines()
        .skip_while(|l| *l != "Shared files between phases:")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .collect();
    assert_eq!(shared, ["  Phase 3 and Phase 4: src/a.rs"], "{}", run.out);
}

#[test]
fn stories_that_share_a_file_are_named() {
    let repo = repo();
    let run = draft(repo.path());
    assert!(
        run.out
            .lines()
            .any(|l| l == "Stories that share files: US1 and US2 (src/a.rs)"),
        "{}",
        run.out
    );
}

#[test]
fn a_task_with_no_path_is_listed_as_changing_no_file() {
    let repo = repo();
    let run = draft(repo.path());
    assert!(
        run.out
            .lines()
            .any(|l| l == "Tasks that change no file: T001, T007"),
        "{}",
        run.out
    );
}

#[test]
fn a_task_whose_files_no_other_task_names_is_listed_as_independent() {
    let repo = repo();
    let run = draft(repo.path());
    let line = run
        .out
        .lines()
        .find(|l| l.starts_with("Tasks whose files no other task names:"))
        .unwrap_or_default();
    assert!(
        line.contains("T006 (apps/copilot-runtime/src/x.ts)"),
        "{}",
        run.out
    );
    assert!(line.contains("T002 (docs/notes.md)"), "{}", run.out);
    // T003 and T005 both name src/a.rs.
    assert!(!line.contains("T003") && !line.contains("T005"), "{line}");
}

#[test]
fn a_ci_paths_glob_that_matches_a_task_path_adds_the_workflow() {
    let repo = repo();
    let run = draft(repo.path());
    let line = entry(&run.out, "US1", ".github/workflows/replay.yml");
    assert!(
        line.is_some_and(|l| l.ends_with("(CI paths glob apps/ai/harness/**)")),
        "{}",
        run.out
    );
    assert!(entry(&run.out, "US2", ".github/workflows/replay.yml").is_none());
}

#[test]
fn a_file_naming_the_dotted_module_is_added() {
    let repo = repo();
    let run = draft(repo.path());
    let line = entry(&run.out, "US1", "apps/ai/.importlinter");
    assert!(
        line.is_some_and(|l| l.ends_with("(names module harness.tools)")),
        "{}",
        run.out
    );
}

#[test]
fn a_test_naming_the_full_file_name_is_added() {
    let repo = repo();
    let run = draft(repo.path());
    let line = entry(&run.out, "US1", "apps/ai/tests/test_targets.py");
    assert!(
        line.is_some_and(|l| l.ends_with("(names file tools.py)")),
        "{}",
        run.out
    );
}

/// Guard: passes on the stub, which prints nothing. The same fixture holds
/// the dotted-module and full-name cases, so a run that lists those proves
/// the command looked at this file too.
#[test]
fn a_file_naming_only_the_bare_stem_is_not_added() {
    let repo = repo();
    let run = draft(repo.path());
    assert!(!run.out.contains("apps/ai/notes.txt"), "{}", run.out);
}

/// Guard: passes on the stub. The feature folder names every path, so it is
/// never a mention, though its plan names `src/a.rs` and `tools.py`.
#[test]
fn the_feature_folder_is_not_a_mention() {
    let repo = repo();
    let run = draft(repo.path());
    assert!(!run.out.contains("specs/001-x/plan.md"), "{}", run.out);
}

#[test]
fn an_unreadable_workflow_is_named_not_skipped() {
    let repo = repo();
    let run = draft(repo.path());
    let line = run
        .out
        .lines()
        .find(|l| l.starts_with("Unread workflows:"))
        .unwrap_or_default();
    assert!(line.contains(".github/workflows/odd.yml ("), "{}", run.out);
}

#[test]
fn a_missing_or_empty_tasks_file_exits_one() {
    let repo = repo();
    let missing = repo.path().join("specs/001-x/none.md");
    let run = teamwire(&["speckit", "draft", missing.to_str().unwrap()]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("none.md"), "{}", run.err);

    let empty = repo.path().join("specs/001-x/empty.md");
    std::fs::write(&empty, "# Tasks\n\nNothing here yet.\n").unwrap();
    let run = teamwire(&["speckit", "draft", empty.to_str().unwrap()]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("empty.md"), "{}", run.err);
    assert!(run.err.contains("no task lines"), "{}", run.err);
}

#[test]
fn the_drafts_are_labelled_as_drafts() {
    let repo = repo();
    let run = draft(repo.path());
    let label = run.out.find(
        "DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.",
    );
    let first = run.out.find("\nUS1:\n");
    assert!(
        label.is_some() && first.is_some() && label < first,
        "{}",
        run.out
    );
    assert!(entry(&run.out, "US1", "src/a.rs").is_some_and(|l| l.ends_with("(task path)")));
    assert!(entry(&run.out, "Setup and polish", "docs/notes.md").is_some());
}
