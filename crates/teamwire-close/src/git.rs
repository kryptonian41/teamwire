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
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
