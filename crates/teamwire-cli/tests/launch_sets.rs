//! `teamwire launch --dry-run` through the real binary prints the set
//! warnings of LCH-34 and still exits 0: every set rule warns, none
//! refuses. `fake-herdr` and `fake-claude` come first on `PATH`.

#![cfg(test)]

use std::path::Path;

use serde_json::{Value, json};
use teamwire_fakes::{FakeHerdrEnv, TestHome};

#[expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary"
)]
fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    std::process::Command::new(program)
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test writes its own fixture files"
)]
fn write_file(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

fn reviewer(role: &str, model: &str, cwd: &Path) -> Value {
    json!({"role": role, "title": role, "kind": "reviewer", "mode": "pane", "cwd": cwd,
           "task": "review the diff", "model": model, "set": "rev1"})
}

/// Two Claude Code reviewers in one set are both Claude models: the
/// harness says so for an alias that the model string alone cannot place.
#[test]
fn dry_run_prints_the_set_warnings_and_exits_0() {
    let herdr = FakeHerdrEnv::new().unwrap();
    let home = TestHome::new().unwrap();
    let work = tempfile::tempdir().unwrap();
    let cwd = work.path();
    let spec = json!({"team": "demo", "title": "Demo team", "children": [
        reviewer("reva", "opus", cwd),
        reviewer("revb", "sonnet", cwd),
        {"role": "rec", "title": "Reconcile", "kind": "reconciler", "mode": "pane",
         "cwd": cwd, "task": "merge the reports", "set": "rev1"},
    ]});
    let file = cwd.join("spec.json");
    write_file(&file, &spec.to_string());
    let out = command(env!("CARGO_BIN_EXE_teamwire"))
        .args(["launch", "--dry-run", "--spec"])
        .arg(&file)
        .env_clear()
        .envs(herdr.env())
        .env("TEAMWIRE_HOME", home.path())
        .env("HERDR_ENV", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    let set_lines: Vec<&str> = err
        .lines()
        .filter(|l| l.starts_with("warning: set_"))
        .collect();
    assert_eq!(
        set_lines,
        [
            "warning: set_same_family: set rev1: reva and revb are both claude models; a set \
             needs a different family per reader",
            "warning: set_reconciler_early: rec reconciles set rev1, but reva has not reported; \
             add the reconciler after every reader has reported",
            "warning: set_reconciler_early: rec reconciles set rev1, but revb has not reported; \
             add the reconciler after every reader has reported",
        ],
        "{err}"
    );
    let created = std::fs::read_dir(home.path()).unwrap().next().is_some();
    assert!(!created, "a dry run creates no state");
}
