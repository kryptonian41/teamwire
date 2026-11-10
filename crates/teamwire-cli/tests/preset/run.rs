//! The speckit preset's commands, run the way launch and verify run them:
//! `sh -c` in a scratch git repository, and `zsh -c` too where zsh is
//! installed, because a lead's shell is often zsh. A test that only matches
//! strings keeps passing when a command stops working, such as a check with
//! `|| true` appended. The fixtures are real output: spec-kit's own
//! `.specify/.gitignore` from `specify init` (spec-kit 1.3.3.dev0), and the
//! finished `tasks.md` of the spec-kit pilot.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{git, teamwire};

/// The skeleton exactly as `teamwire preset speckit` prints it.
fn printed() -> Value {
    let run = teamwire(&["preset", "speckit"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let (_, skeleton) = run
        .out
        .split_once("`teamwire launch --spec FILE`:\n")
        .unwrap();
    serde_json::from_str(skeleton).unwrap()
}

fn member<'a>(doc: &'a Value, role: &str) -> &'a Value {
    doc["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == role)
        .unwrap_or_else(|| unreachable!("no member {role}"))
}

/// The feature folder the skeleton names, such as `specs/001-CHANGEME`.
fn folder(doc: &Value) -> String {
    format!("specs/{}", doc["refs"][0]["id"].as_str().unwrap())
}

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/preset")
            .join(name),
    )
    .unwrap()
}

/// `sh`, and `zsh` when it is installed.
fn shells() -> Vec<&'static str> {
    let zsh = std::process::Command::new("zsh")
        .args(["-c", "true"])
        .status()
        .is_ok_and(|s| s.success());
    if zsh { vec!["sh", "zsh"] } else { vec!["sh"] }
}

/// A repository laid out as spec-kit leaves it, with the feature folder
/// committed and `tasks.md` left out, so each case writes its own.
fn repo(work: &Path, feature: &str) -> PathBuf {
    let repo = work.join("repo");
    std::fs::create_dir_all(repo.join(".specify")).unwrap();
    std::fs::create_dir_all(repo.join(feature)).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), ".worktrees/\n").unwrap();
    std::fs::write(
        repo.join(".specify/.gitignore"),
        fixture_text("specify.gitignore"),
    )
    .unwrap();
    std::fs::write(repo.join(feature).join("spec.md"), "# Spec\n").unwrap();
    std::fs::write(repo.join(feature).join("plan.md"), "# Plan\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "spec-kit"]);
    repo
}

/// Exit code of `<shell> -c <script> teamwire-setup <dir>` run in `dir`, the
/// form launch runs a `setup` in and a superset of how verify runs a check.
fn run_in(shell: &str, script: &str, dir: &Path) -> i32 {
    std::process::Command::new(shell)
        .arg("-c")
        .arg(script)
        .arg("teamwire-setup")
        .arg(dir)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
        .status
        .code()
        .unwrap_or(-1)
}

/// The implementer's check, with its test command placeholder replaced.
fn impl_check(doc: &Value, tests: &str) -> String {
    let check = member(doc, "impl")["check"].as_str().unwrap();
    let placeholder = "CHANGEME targeted test command";
    assert_eq!(check.matches(placeholder).count(), 1, "{check}");
    check.replace(placeholder, tests)
}

/// Every open task shape a model may write fails the implementer's check.
/// Two shapes still pass, and the guidance says so: a lowercase `[x]` and a
/// deleted line, which the closer finds by counting the ticks.
#[test]
fn the_speckit_implementer_check_fails_on_every_open_task() {
    let doc = printed();
    let feature = folder(&doc);
    let work = tempfile::tempdir().unwrap();
    let repo = repo(work.path(), &feature);
    let tasks = repo.join(&feature).join("tasks.md");
    let done = fixture_text("tasks_done.md");
    let t006 = "- [X] T006 [US1]";
    assert!(done.contains(t006) && !done.contains("- [ ] "), "{done}");
    let open = |line: &str| Some(done.replace(t006, line));
    let crlf = done.replace(t006, "- [ ] T006 [US1]").replace('\n', "\r\n");
    let deleted = done
        .lines()
        .filter(|l| !l.starts_with(t006))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let converge = format!(
        "{done}\n## Phase 8: Convergence\n\n- [ ] T020 Add the missing case per FR-004 (missing)\n"
    );
    let cases: Vec<(&str, Option<String>, &str, bool)> = vec![
        ("every task ticked", Some(done.clone()), "true", true),
        ("the tests fail", Some(done.clone()), "false", false),
        ("one open task", open("- [ ] T006 [US1]"), "true", false),
        ("no tasks.md", None, "true", false),
        ("an empty tasks.md", Some(String::new()), "true", false),
        ("CRLF line endings", Some(crlf), "true", false),
        ("converge's open task", Some(converge), "true", false),
        (
            "two spaces after the dash",
            open("-  [ ] T006 [US1]"),
            "true",
            false,
        ),
        (
            "two spaces before the id",
            open("- [ ]  T006 [US1]"),
            "true",
            false,
        ),
        (
            "a tab before the id",
            open("- [ ]\tT006 [US1]"),
            "true",
            false,
        ),
        (
            "an indented task",
            open("  - [ ] T006 [US1]"),
            "true",
            false,
        ),
        ("a star bullet", open("* [ ] T006 [US1]"), "true", false),
        ("a plus bullet", open("+ [ ] T006 [US1]"), "true", false),
        (
            "two spaces in the box",
            open("- [  ] T006 [US1]"),
            "true",
            false,
        ),
        ("a lowercase tick", open("- [x] T006 [US1]"), "true", true),
        ("a deleted task line", Some(deleted), "true", true),
    ];
    for shell in shells() {
        for (case, text, tests, passes) in &cases {
            match text {
                Some(text) => std::fs::write(&tasks, text).unwrap(),
                None => std::fs::remove_file(&tasks).unwrap_or_default(),
            }
            let code = run_in(shell, &impl_check(&doc, tests), &repo);
            assert_eq!(
                code == 0,
                *passes,
                "{shell}: {case}: the check exited {code}"
            );
        }
    }
}

/// The spec member's check fails until `tasks` has written `tasks.md`, so at
/// a question stop it records `blocked`. It owns every file spec-kit writes
/// in the feature folder.
#[test]
fn the_speckit_spec_check_waits_for_tasks_and_owns_the_whole_folder() {
    let doc = printed();
    let feature = folder(&doc);
    let spec = member(&doc, "spec");
    let check = spec["check"].as_str().unwrap();
    let work = tempfile::tempdir().unwrap();
    let repo = repo(work.path(), &feature);
    for shell in shells() {
        std::fs::remove_file(repo.join(&feature).join("tasks.md")).unwrap_or_default();
        assert_ne!(run_in(shell, check, &repo), 0, "{shell}: no tasks.md yet");
        std::fs::write(repo.join(&feature).join("tasks.md"), "# Tasks\n").unwrap();
        assert_eq!(run_in(shell, check, &repo), 0, "{shell}: tasks.md written");
    }
    let owns: Vec<&str> = spec["owns"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for file in [
        "spec.md",
        "plan.md",
        "tasks.md",
        "research.md",
        "data-model.md",
        "quickstart.md",
        "contracts/cli.md",
        "checklists/requirements.md",
    ] {
        let path = format!("{feature}/{file}");
        assert!(
            owns.iter()
                .any(|o| *o == path || (o.ends_with('/') && path.starts_with(o))),
            "the spec member does not own {path}: {owns:?}"
        );
    }
}

/// Every `setup` of every preset, the team's and each member's, as printed.
fn setups(name: &str) -> Vec<String> {
    let run = teamwire(&["preset", name]);
    assert_eq!(run.code, 0, "{name}: {}", run.err);
    let (_, skeleton) = run
        .out
        .split_once("`teamwire launch --spec FILE`:\n")
        .unwrap();
    let doc: Value = serde_json::from_str(skeleton).unwrap();
    std::iter::once(&doc)
        .chain(doc["children"].as_array().unwrap())
        .filter_map(|v| v["setup"].as_str().map(str::to_owned))
        .collect()
}

/// A lead fills in only what the skeleton marks, so every `setup` runs as
/// printed in a repository that has no script of its own, and the fill-in
/// line names the `setup` placeholder. Launch runs `setup` in each new
/// worktree, and a dry run never does, so a missing script fails only at
/// the real launch, with exit 127.
#[test]
fn every_preset_setup_runs_as_printed_and_is_marked_to_fill_in() {
    let run = teamwire(&["preset", "speckit"]);
    let fill_in = run
        .out
        .lines()
        .find(|l| l.starts_with("Spec skeleton."))
        .unwrap();
    let placeholder = "true CHANGEME worktree setup command";
    assert!(fill_in.contains(&format!("`{placeholder}`")), "{fill_in}");
    let work = tempfile::tempdir().unwrap();
    let repo = repo(work.path(), "specs/001-CHANGEME");
    for name in super::NAMES {
        for setup in setups(name) {
            assert!(setup.starts_with(placeholder), "{name}: {setup}");
            for shell in shells() {
                assert_eq!(run_in(shell, &setup, &repo), 0, "{shell}: {name}: {setup}");
            }
        }
    }
}

/// The implementer's `setup` writes the pointer spec-kit's scripts read,
/// with the one key they read, and git ignores the file, so the member
/// never commits it.
#[test]
fn the_speckit_implementer_setup_writes_the_ignored_feature_pointer() {
    let doc = printed();
    let feature = folder(&doc);
    let setup = member(&doc, "impl")["setup"].as_str().unwrap();
    let work = tempfile::tempdir().unwrap();
    let repo = repo(work.path(), &feature);
    let pointer = repo.join(".specify/feature.json");
    for shell in shells() {
        std::fs::remove_file(&pointer).unwrap_or_default();
        assert_eq!(run_in(shell, setup, &repo), 0, "{shell}: {setup}");
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&pointer).unwrap())
            .unwrap_or_else(|e| unreachable!("{shell}: feature.json is not JSON: {e}"));
        assert_eq!(
            written,
            serde_json::json!({ "feature_directory": feature }),
            "{shell}"
        );
        git(&repo, &["check-ignore", "-q", ".specify/feature.json"]);
    }
}
