//! `scripts/test-changed.sh --dry-run --file PATH`: which workspace crates a
//! changed file tests. A crate tests a file of its own folder, and a file
//! outside its folder that one of its string literals names
//! (docs/conventions.md section 2). The script runs from the repository root
//! and builds nothing.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts scripts/test-changed.sh"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "helper functions in an integration-test file fail the test when the script does not run"
)]

use std::path::{Path, PathBuf};

/// The script, named by a path relative to this crate. The relative path is
/// also what makes the script itself test this crate.
fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/test-changed.sh")
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

/// Runs the script with `--dry-run` and one `--file` per path, from the
/// repository root.
fn dry_run(files: &[&str]) -> Run {
    let script = script();
    let root = script
        .parent()
        .and_then(Path::parent)
        .expect("the script sits two folders below the root");
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(&script).arg("--dry-run").current_dir(root);
    for f in files {
        cmd.arg("--file").arg(f);
    }
    let out = cmd.output().expect("bash runs");
    Run {
        code: out.status.code().expect("the script exits"),
        out: String::from_utf8(out.stdout).expect("utf-8"),
        err: String::from_utf8(out.stderr).expect("utf-8"),
    }
}

/// The crates on the script's `test-changed: changed:` line.
fn changed(file: &str) -> Vec<String> {
    let run = dry_run(&[file]);
    assert_eq!(run.code, 0, "{file}: {}{}", run.out, run.err);
    let line = run
        .out
        .lines()
        .find_map(|l| l.strip_prefix("test-changed: changed: "))
        .unwrap_or_else(|| panic!("{file}: no changed line in {}{}", run.out, run.err));
    line.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn the_model_evidence_tests_the_cli_and_core() {
    assert_eq!(
        changed("examples/models/evidence.json"),
        ["teamwire-cli", "teamwire-core"]
    );
}

/// The CLI's tests name the folder `examples/` and read the skill below it,
/// so the CLI is one of the five.
#[test]
fn the_shared_skill_tests_every_crate_that_reads_it() {
    assert_eq!(
        changed("examples/skills/teamwire/SKILL.md"),
        [
            "teamwire-cli",
            "teamwire-deliver",
            "teamwire-harness-claude",
            "teamwire-harness-pi",
            "teamwire-launch"
        ]
    );
}

#[test]
fn a_preset_tests_the_cli() {
    assert_eq!(changed("examples/presets/survey.json"), ["teamwire-cli"]);
}

#[test]
fn the_mcp_reference_tests_the_cli() {
    assert_eq!(changed("docs/api/mcp.md"), ["teamwire-cli"]);
}

#[test]
fn a_doc_no_crate_reads_tests_nothing() {
    let run = dry_run(&["docs/api/cli.md"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out
            .contains("test-changed: no changed file belongs to a workspace crate"),
        "{}",
        run.out
    );
    assert!(!run.out.contains("test-changed: changed:"), "{}", run.out);
}

/// A fixture of one crate that another crate's test embeds tests both.
#[test]
fn a_fixture_another_crate_embeds_tests_both_crates() {
    assert_eq!(
        changed("crates/teamwire-harness-pi/tests/fixtures/list-models.txt"),
        ["teamwire-harness-pi", "teamwire-launch"]
    );
}

#[test]
fn a_file_flag_without_a_path_is_a_usage_error() {
    let script = script();
    let out = std::process::Command::new("bash")
        .arg(&script)
        .args(["--dry-run", "--file"])
        .output()
        .expect("bash runs");
    assert_eq!(out.status.code(), Some(3));
}

/// A file of the repository that does not exist, named by a relative path
/// as a reader of another crate's file names it. Under `docs/`, no crate
/// reads a folder that holds it.
const DELETED: &str = "../../docs/test-changed-deleted-file.md";

/// A deleted or renamed file still tests the crate that names it: the script
/// matches the old path, which no longer exists.
#[test]
fn a_deleted_file_tests_the_crate_that_names_it() {
    let path = DELETED.trim_start_matches("../../");
    let root = script()
        .parent()
        .and_then(Path::parent)
        .expect("the script sits two folders below the root")
        .join(path);
    assert!(!root.exists(), "{} must not exist", root.display());
    assert_eq!(changed(path), ["teamwire-cli"]);
}

/// A change to the root `Cargo.toml` can change how every crate builds, so
/// the script says to run the full gate even when it also tests crates.
#[test]
fn a_root_build_file_asks_for_the_full_gate() {
    let run = dry_run(&["Cargo.toml"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.contains(
            "test-changed: a change to the root Cargo.toml, Cargo.lock or toolchain needs scripts/gate.sh"
        ),
        "{}",
        run.out
    );
}

/// The CLI's dialog tests read a fixture of the Claude Code adapter through
/// the format string `"{}/../teamwire-harness-claude/..."`, with the crate's
/// folder in the `{}`.
#[test]
fn a_fixture_named_through_a_format_string_tests_its_reader() {
    assert_eq!(
        changed(
            "crates/teamwire-harness-claude/tests/fixtures/permission/bash-rmdir-countdown.txt"
        ),
        ["teamwire-cli", "teamwire-harness-claude"]
    );
}
