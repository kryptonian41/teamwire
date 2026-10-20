//! git, always through `Platform::run`, and never with a force flag (CLS-7).

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Command, Platform, RunOutput};

/// Git on a small repository answers in milliseconds; two minutes bounds a
/// hung network filesystem.
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Runs `git -C <dir> <args>`. An error is the text a person can read.
pub(crate) fn run(platform: &dyn Platform, dir: &Path, args: &[&str]) -> Result<RunOutput, String> {
    let cmd = Command::new("git")
        .arg("-C")
        .arg(dir.display().to_string())
        .args(args.iter().copied())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    platform
        .run(&cmd, GIT_TIMEOUT)
        .map_err(|e| format!("git {}: {e}", args.join(" ")))
}

/// The text a failed call left, for a message.
pub(crate) fn failure_text(out: &RunOutput) -> String {
    out.failure_text()
}

/// The trimmed standard output of a call that must succeed.
pub(crate) fn stdout_of(
    platform: &dyn Platform,
    dir: &Path,
    args: &[&str],
) -> Result<String, String> {
    let out = run(platform, dir, args)?;
    if !out.success() {
        return Err(format!("git {}: {}", args.join(" "), failure_text(&out)));
    }
    Ok(out.stdout_trimmed())
}

/// True only when git answers that `branch` is not in `repo`. A repository
/// or git that cannot answer counts as "still there", so a kept branch is
/// never left out of the report by a failed check.
pub(crate) fn branch_is_gone(platform: &dyn Platform, repo: &Path, branch: &str) -> bool {
    let name = format!("refs/heads/{branch}");
    run(platform, repo, &["rev-parse", "--verify", "--quiet", &name])
        .is_ok_and(|out| !out.success() && out.exit_code == 1)
}
