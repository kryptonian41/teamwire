//! git, always through `Platform::run` so a test can replace it.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Command, Platform, RunOutput};

/// How long one git call may take.
pub(crate) const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Runs `git -C <dir> <args>`. An error is the text a person can read.
pub(crate) fn run(platform: &dyn Platform, dir: &Path, args: &[&str]) -> Result<RunOutput, String> {
    let cmd = Command::new("git")
        .arg("-C")
        .arg(dir.display().to_string())
        .args(args.iter().copied());
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
