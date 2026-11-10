//! The spec-kit checks of `teamwire doctor`, printed only when doctor runs
//! inside a git repository whose root holds `.specify/`.
//!
//! Never: run `specify`, use the network, or change a file. The checks read
//! files and ask git (`ls-files`, `check-ignore`).
//! Every line is `ok` or `warn`, never `FAIL`: a doctor failure blocks
//! `teamwire afk start`, and none of these stops a launch on its own. Each
//! warning names the file and what to do.

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::Command;

use crate::cmd::doctor::{Check, Inputs};

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

const CONSTITUTION: &str = ".specify/memory/constitution.md";
const FEATURE_POINTER: &str = ".specify/feature.json";

/// The spec-kit checks, or none outside a spec-kit repository.
pub(crate) fn speckit_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let Some(root) = speckit_root(&inputs.repo) else {
        return Vec::new();
    };
    vec![
        committed_check(inputs, &root),
        feature_pointer_check(inputs, &root),
    ]
}

/// The repository root is the nearest directory at or above `dir` that
/// holds `.git` (a directory, or a file in a worktree). It counts only when
/// it also holds `.specify/`.
fn speckit_root(dir: &Path) -> Option<PathBuf> {
    let root = dir.ancestors().find(|d| d.join(".git").exists())?;
    root.join(".specify").is_dir().then(|| root.to_path_buf())
}

/// Runs `git args` at the root and returns its exit code, or why it gave none.
fn git(inputs: &Inputs<'_>, root: &Path, args: &[&str]) -> Result<i32, String> {
    let cmd = Command::new("git")
        .args(args.iter().copied())
        .cwd(root.to_path_buf());
    match inputs.platform.run(&cmd, GIT_TIMEOUT) {
        Ok(out) if out.timed_out => Err(format!("no answer within {} s", GIT_TIMEOUT.as_secs())),
        Ok(out) => Ok(out.exit_code),
        Err(e) => Err(e.to_string()),
    }
}

/// A worktree holds only what is committed, so an untracked `.specify/`
/// leaves a worktree member with no spec-kit files.
fn committed_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit committed";
    match git(
        inputs,
        root,
        &["ls-files", "--error-unmatch", "--", CONSTITUTION],
    ) {
        Ok(0) => Check::pass(name, format!("{CONSTITUTION} is tracked by git")),
        Ok(1) => Check::warn(
            name,
            format!(
                "{CONSTITUTION} is not tracked by git, so a worktree member has no spec-kit files; \
                 commit .specify/ and the agents' spec-kit command folders"
            ),
        ),
        Ok(code) => Check::warn(name, format!("git ls-files exited with code {code}")),
        Err(e) => Check::warn(name, format!("could not run git: {e}")),
    }
}

/// spec-kit rewrites its feature pointer on every feature switch, so each
/// member's copy differs and must stay out of commits.
fn feature_pointer_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit feature pointer";
    match git(inputs, root, &["check-ignore", "-q", FEATURE_POINTER]) {
        Ok(0) => Check::pass(name, format!("git ignores {FEATURE_POINTER}")),
        Ok(1) => Check::warn(
            name,
            format!(
                "git does not ignore {FEATURE_POINTER}, so members would commit their feature \
                 pointer; add `feature.json` to .specify/.gitignore"
            ),
        ),
        Ok(code) => Check::warn(name, format!("git check-ignore exited with code {code}")),
        Err(e) => Check::warn(name, format!("could not run git: {e}")),
    }
}
