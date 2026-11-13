//! Which words of a task line `teamwire speckit draft` reads as paths
//! (FR-013, ADR 0044 decision 2), on the line shapes of deckh's `tasks.md`:
//! a route, an event name, an absolute path, a markdown link, a folder, a
//! root file and a brace set.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and makes a throwaway git repository"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::Path;

/// deckh's T051 names a route in quotes beside its three files, and T054
/// names the same route, a module and two thread prefixes. The other tasks
/// each name one kind of word in two phases, so a word read as a path makes
/// a phase pair.
const TASKS: &str = "# Tasks: deckh

## Phase 1: Setup

- [ ] T001 Read `AGENTS.md` and `README.md`, then [the model](docs/data-model.md).
- [ ] T002 Check `/Users/someone/Projects/teamwire` and `lib.rs`.

## Phase 2: User Story 1 - first (Priority: P1)

- [ ] T010 [US1] Emit `knit.progress` from `apps/ai/harness/tools.py`.
- [ ] T011 [US1] Keep `apps/ai` tidy.

## Phase 3: User Story 2 - second (Priority: P2)

- [ ] T051 [P] [US2] Add `knit_deck_editor_harness: \"/agui/deck_editor\"` to `AGENT_PATHS` in `apps/copilot-runtime/src/routes/agent-paths.ts`, with the cases in `apps/copilot-runtime/src/routes/agent-paths.test.ts` and `apps/copilot-runtime/src/app.test.ts`.
- [ ] T054 [US2] Write `apps/ai/gateway/decks/tests/test_contract.py`: `api.py` does not import `gateway.decks` and does not mount `/agui/deck_editor`; the thread prefix `deck_editor/` does not start with `deck/`.
- [ ] T055 [US2] Wire `knit.progress` into `src/{a,b}.rs`.

## Phase 4: Polish

- [ ] T060 Update `README.md`, see [the model](docs/data-model.md), and `/Users/someone/Projects/teamwire`, `lib.rs`.
- [ ] T061 Change `src/b.rs`.
";

/// Lines of 002's `tasks.md` that gave wrong `owns` entries: code in
/// backticks, example globs, partial paths and a template path (ADR 0047).
const TASKS_002: &str = "# Tasks: 002

## Phase 3: User Story 1 - amend (Priority: P1)

- [X] T002 [P] [US1] Call `tx.set_member_scope(&role, Some(\"b\"), &[\"y/\".to_owned(), \"z/{c,d}/**\".to_owned()])` in `crates/teamwire-store/src/member.rs`.
- [X] T005 [US1] Keep the callers (`crates/teamwire-cli/src/cmd/send.rs:86`).
- [X] T010 [US1] Write the tests:
  - `amend_keeps_a_brace_glob_whole`: `--owns src/ --owns 'crates/{b,c}/**'` exits 0.
  - refuse as `cmd/verify.rs:24-31` does, as `contracts/amend-cli.md` says; the rendered `teamwire/skills/teamwire/SKILL.md` and the package file `skills/teamwire/SKILL.md` name `teamwire amend`.
- [X] T011 [US1] Launch with check `test -s specs/NNN-name/tasks.md`, then write `crates/teamwire-cli/src/cmd/amend.rs` (new) and change `cmd/lib.rs`.
- [X] T018 [US1] Read `examples/skills/teamwire/SKILL.md` from `Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"../../examples/skills/teamwire/SKILL.md\")`.
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

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // T051's three files are tracked, as they are in deckh's repository,
    // so they resolve (ADR 0047).
    let files: [(&str, &str); 12] = [
        ("specs/001-deckh/tasks.md", TASKS),
        (
            "apps/copilot-runtime/src/routes/agent-paths.ts",
            "export {};\n",
        ),
        (
            "apps/copilot-runtime/src/routes/agent-paths.test.ts",
            "test();\n",
        ),
        ("apps/copilot-runtime/src/app.test.ts", "test();\n"),
        ("README.md", "# deckh\n"),
        ("AGENTS.md", "rules\n"),
        ("docs/data-model.md", "the model\n"),
        ("apps/ai/harness/tools.py", "def tool(): pass\n"),
        (
            "apps/ai/README.md",
            "Everything under apps/ai is the AI service.\n",
        ),
        ("crates/x/src/lib.rs", "// lib\n"),
        ("src/a.rs", "fn a() {}\n"),
        ("src/b.rs", "fn b() {}\n"),
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
    let tasks = repo.join("specs/001-deckh/tasks.md");
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

/// deckh's T051 shares no file with T054: the route both name is no path.
#[test]
fn a_route_in_quotes_is_no_path_so_t051_is_independent() {
    let repo = repo();
    let out = draft(repo.path());
    let alone = line(&out, "Tasks whose files no other task names:");
    assert!(
        alone.contains(
            "T051 (apps/copilot-runtime/src/routes/agent-paths.ts, \
             apps/copilot-runtime/src/routes/agent-paths.test.ts, apps/copilot-runtime/src/app.test.ts)"
        ),
        "{out}"
    );
    assert!(!out.contains("/agui/deck_editor"), "{out}");
}

/// An event name, a module name, an absolute path, a bare file name that
/// no root file has, and a markdown link's target are no paths, so none of
/// them pairs two phases.
#[test]
fn names_absolute_paths_and_link_targets_are_no_paths() {
    let repo = repo();
    let out = draft(repo.path());
    for word in [
        "knit.progress",
        "gateway.decks",
        "/Users/someone",
        "lib.rs",
        "docs/data-model.md",
        "api.py",
        "deck_editor/",
    ] {
        assert!(!out.contains(word), "{word}: {out}");
    }
}

/// A word with no `/` that names a file at the repository's root is a path.
#[test]
fn a_root_file_is_a_path() {
    let repo = repo();
    let out = draft(repo.path());
    let shared = block(&out, "Shared files between phases:");
    assert!(
        shared.contains(&"  Phase 1 and Phase 4: README.md"),
        "{out}"
    );
    let setup = block(&out, "Setup and polish:");
    assert!(setup.iter().any(|l| l.starts_with("  AGENTS.md ")), "{out}");
}

/// A folder pairs with the files under it in the overlap table, and adds no
/// file that merely mentions the folder to the drafts.
#[test]
fn a_folder_is_a_prefix_of_its_files_and_adds_no_mention() {
    let repo = repo();
    let out = draft(repo.path());
    let us1 = block(&out, "US1:");
    assert!(us1.iter().any(|l| l.starts_with("  apps/ai/ ")), "{out}");
    assert!(!out.contains("apps/ai/README.md"), "{out}");
    let alone = line(&out, "Tasks whose files no other task names:");
    assert!(!alone.contains("T010") && !alone.contains("T011"), "{out}");
}

/// A brace set names each of its files.
#[test]
fn a_brace_set_names_each_file() {
    let repo = repo();
    let out = draft(repo.path());
    let shared = block(&out, "Shared files between phases:");
    assert!(shared.contains(&"  Phase 3 and Phase 4: src/b.rs"), "{out}");
    let us2 = block(&out, "US2:");
    assert!(us2.iter().any(|l| l.starts_with("  src/a.rs ")), "{out}");
}

/// The 002 fixture: the files the partial paths end, two files that both
/// end `cmd/lib.rs`, a tracked `z/c/` for the code glob, and an archived
/// study under `bench/` that names a task path.
fn repo_002() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let files: [(&str, &str); 11] = [
        ("specs/002-x/tasks.md", TASKS_002),
        ("specs/002-x/contracts/amend-cli.md", "# amend\n"),
        ("crates/teamwire-store/src/member.rs", "// member\n"),
        ("crates/teamwire-cli/src/cmd/send.rs", "// send\n"),
        ("crates/teamwire-cli/src/cmd/verify.rs", "// verify\n"),
        ("crates/p/src/cmd/lib.rs", "// p\n"),
        ("crates/q/src/cmd/lib.rs", "// q\n"),
        ("examples/skills/teamwire/SKILL.md", "# skill\n"),
        ("z/c/one.rs", "// one\n"),
        (
            "bench/studies/s1/diff.patch",
            "+++ b/crates/teamwire-store/src/member.rs\n",
        ),
        (
            "docs/store.md",
            "See crates/teamwire-store/src/member.rs.\n",
        ),
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

fn draft_002(repo: &Path, extra: &[&str]) -> String {
    let home = tempfile::tempdir().unwrap();
    let tasks = repo.join("specs/002-x/tasks.md");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(["speckit", "draft", tasks.to_str().unwrap()])
        .args(extra)
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

/// The first word of each line of the DRAFT list `name`.
fn draft_paths<'a>(out: &'a str, name: &str) -> Vec<&'a str> {
    block(out, &format!("{name}:"))
        .into_iter()
        .filter_map(|l| l.split_whitespace().next())
        .collect()
}

/// ADR 0047 rule 1: code in backticks is no path, so no draft holds it.
#[test]
fn code_in_backticks_is_no_task_path() {
    let repo = repo_002();
    let out = draft_002(repo.path(), &[]);
    let us1 = draft_paths(&out, "US1");
    assert!(
        us1.contains(&"crates/teamwire-store/src/member.rs"),
        "{out}"
    );
    for word in us1 {
        assert!(
            !word.contains("to_owned") && !word.contains("CARGO_MANIFEST_DIR"),
            "{word}: {out}"
        );
    }
}

/// ADR 0047 rule 2: an example glob that matches no tracked file and a
/// template path are no task paths. A new file in a tracked folder is.
#[test]
fn a_task_path_must_resolve_in_the_repository() {
    let repo = repo_002();
    let out = draft_002(repo.path(), &[]);
    let us1 = draft_paths(&out, "US1");
    assert!(
        us1.contains(&"crates/teamwire-cli/src/cmd/amend.rs"),
        "{out}"
    );
    for word in ["crates/b/**", "crates/c/**", "specs/NNN-name/tasks.md"] {
        assert!(!us1.contains(&word), "{word}: {out}");
    }
}
