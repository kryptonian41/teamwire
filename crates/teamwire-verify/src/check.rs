//! Running a member's `check` (VER-1, INV-29): the real exit code, the full
//! output in `verify/<role>.log`, a timeout as exit 124.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use teamwire_core::{Command, Member, RunOutput};
use teamwire_store::Ctx;

/// What one check run left behind.
#[derive(Debug)]
pub(crate) struct CheckOutcome {
    pub(crate) exit_code: i32,
    pub(crate) seconds: u64,
    /// The log, and the SHA-256 the store keeps of it. `None` when the log
    /// could not be written; the exit code is still real.
    pub(crate) log: Option<(PathBuf, [u8; 32])>,
    /// Why the check did not run or its log was not saved.
    pub(crate) note: Option<String>,
}

/// Runs `check` through `Platform::run` in the member's directory. The
/// platform starts it in its own process group and kills the group when the
/// member's timeout passes. A check that could not start at all is exit 127,
/// as a shell reports a missing command.
pub(crate) fn run_check(
    ctx: &Ctx<'_>,
    member: &Member,
    check: &str,
    team_dir: &Path,
) -> CheckOutcome {
    let timeout = Duration::from_secs(if member.deadline_s == 0 {
        ctx.config.default_deadline_s
    } else {
        member.deadline_s
    });
    let cmd = Command::new("sh").args(["-c", check]).cwd(&member.cwd);
    let started = ctx.clock.monotonic();
    let result = ctx.platform.run(&cmd, timeout);
    let seconds = ctx
        .clock
        .monotonic()
        .saturating_duration_since(started)
        .as_secs();
    let (exit_code, mut text, mut note) = match result {
        Ok(out) => {
            let text = log_text(check, &member.cwd, &out, timeout);
            (out.exit_code, text, None)
        }
        Err(e) => (
            127,
            format!(
                "$ {check}   # in {}\n\n[teamwire verify] could not run: {e}\n",
                member.cwd.display()
            ),
            Some(format!("the check could not be run: {e}")),
        ),
    };
    let dir = team_dir.join("verify");
    let path = dir.join(format!("{}.log", member.role.as_str()));
    let written = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            ctx.platform
                .write_atomic(&path, text.as_bytes())
                .map_err(|e| e.to_string())
        });
    let log = match written {
        Ok(()) => {
            text.shrink_to_fit();
            Some((path, Sha256::digest(text.as_bytes()).into()))
        }
        Err(e) => {
            note.get_or_insert_with(|| format!("the log was not saved: {e}"));
            None
        }
    };
    CheckOutcome {
        exit_code,
        seconds,
        log,
        note,
    }
}

fn log_text(check: &str, cwd: &Path, out: &RunOutput, timeout: Duration) -> String {
    let mut text = format!("$ {check}   # in {}\n", cwd.display());
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    if !out.stderr.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("--- stderr ---\n");
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    if out.timed_out {
        text.push_str("\n[teamwire verify] timed out after ");
        text.push_str(&timeout.as_secs().to_string());
        text.push_str("s\n");
    }
    text
}
