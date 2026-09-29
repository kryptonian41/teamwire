//! One tmux call: run `tmux [-L socket] <args>` through `Platform::run` with
//! a timeout and turn what came back into text or a failure.
//!
//! tmux has no error codes, only a message on stderr and exit 1, so this is
//! the one place that reads that text. It looks for the two phrases tmux has
//! printed for a missing server and a missing pane, window or session, and
//! treats every other failure as `Other`.

use std::time::Duration;

use teamwire_core::{Command, HostError, HostErrorCode, Platform};

/// Longest tmux text copied into an error detail.
const DETAIL_MAX: usize = 300;

/// Why a tmux call gave no output.
#[derive(Debug)]
pub(crate) enum Failure {
    /// No tmux server answers on the socket.
    NoServer(String),
    /// The named pane, window or session does not exist.
    NoTarget(String),
    Host(HostError),
}

impl From<Failure> for HostError {
    fn from(f: Failure) -> Self {
        match f {
            Failure::NoServer(d) | Failure::NoTarget(d) => Self::new(HostErrorCode::NotFound, d),
            Failure::Host(e) => e,
        }
    }
}

/// Cuts `text` to at most `DETAIL_MAX` characters.
pub(crate) fn clip(text: &str) -> String {
    text.trim().chars().take(DETAIL_MAX).collect()
}

/// Runs tmux on `socket` (the default server when `None`) and returns its
/// standard output.
pub(crate) fn call(
    platform: &dyn Platform,
    socket: Option<&str>,
    args: &[&str],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<String, Failure> {
    let mut cmd = Command::new("tmux");
    if let Some(name) = socket {
        cmd = cmd.args(["-L", name]);
    }
    cmd = cmd.args(args.iter().copied());
    cmd.stdin = stdin.map(<[u8]>::to_vec);
    let out = platform.run(&cmd, timeout).map_err(|e| {
        Failure::Host(HostError::new(
            HostErrorCode::Other,
            format!("could not run tmux: {}", clip(&e.to_string())),
        ))
    })?;
    if out.timed_out {
        return Err(Failure::Host(HostError::new(
            HostErrorCode::Timeout,
            "tmux did not answer in time",
        )));
    }
    if out.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail = clip(&stderr);
    if detail.contains("no server running") || detail.contains("error connecting to") {
        return Err(Failure::NoServer(detail));
    }
    if detail.contains("can't find") {
        return Err(Failure::NoTarget(detail));
    }
    Err(Failure::Host(HostError::new(
        HostErrorCode::Other,
        format!("tmux exited {}: {detail}", out.exit_code),
    )))
}

#[cfg(test)]
mod tests;
