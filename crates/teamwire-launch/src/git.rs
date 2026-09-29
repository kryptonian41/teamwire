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
    if out.timed_out {
        return "timed out".to_owned();
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let text = text.trim();
    if text.is_empty() {
        format!("exit {}", out.exit_code)
    } else {
        text.to_owned()
    }
}
