//! One herdr call: run `herdr` through `Platform::run` with a timeout and
//! turn what came back into a result value or a `HostError`.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use teamwire_core::{Command, HostError, HostErrorCode, Platform, RunOutput};

use crate::errors::{ErrorBody, clip, from_body};

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ErrorBody>,
}

fn envelope(bytes: &[u8]) -> Option<Envelope> {
    serde_json::from_slice(bytes).ok()
}

fn text(bytes: &[u8]) -> String {
    clip(&String::from_utf8_lossy(bytes))
}

/// Runs `herdr <args>` and returns `.result`. The error envelope is on
/// stderr with exit 1; a syntax error is usage text on stderr with exit 2.
pub(crate) fn call(
    platform: &dyn Platform,
    args: &[String],
    timeout: Duration,
) -> Result<Value, HostError> {
    interpret(&run(platform, args, timeout)?)
}

/// Runs `herdr <args>` for a command that prints plain text when it succeeds.
/// `agent read` and `pane read` print the screen itself, with no JSON
/// envelope around it and no option to ask for one. They fail the same way
/// as every other command.
pub(crate) fn call_text(
    platform: &dyn Platform,
    args: &[String],
    timeout: Duration,
) -> Result<String, HostError> {
    interpret_text(&run(platform, args, timeout)?)
}

fn run(
    platform: &dyn Platform,
    args: &[String],
    timeout: Duration,
) -> Result<RunOutput, HostError> {
    let cmd = Command::new("herdr").args(args.iter().cloned());
    platform.run(&cmd, timeout).map_err(|e| {
        HostError::new(
            HostErrorCode::Other,
            format!("could not run herdr: {}", clip(&e.to_string())),
        )
    })
}

fn interpret(out: &RunOutput) -> Result<Value, HostError> {
    if !out.success() {
        return Err(failure(out));
    }
    let parsed = envelope(&out.stdout).or_else(|| envelope(&out.stderr));
    if let Some(body) = parsed.as_ref().and_then(|e| e.error.as_ref()) {
        return Err(from_body(body));
    }
    parsed.and_then(|e| e.result).ok_or_else(|| {
        HostError::new(
            HostErrorCode::Other,
            format!("herdr printed no result: {}", text(&out.stdout)),
        )
    })
}

fn interpret_text(out: &RunOutput) -> Result<String, HostError> {
    if !out.success() {
        return Err(failure(out));
    }
    // The screen can itself show JSON, even an error envelope, so the text
    // of a successful read is never parsed.
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The error for a call that timed out or exited non-zero.
fn failure(out: &RunOutput) -> HostError {
    if out.timed_out {
        return HostError::new(HostErrorCode::Timeout, "herdr did not answer in time");
    }
    let parsed = envelope(&out.stdout).or_else(|| envelope(&out.stderr));
    if let Some(body) = parsed.as_ref().and_then(|e| e.error.as_ref()) {
        return from_body(body);
    }
    if out.exit_code == 2 {
        return HostError::new(
            HostErrorCode::BadArgument,
            format!("herdr usage error: {}", text(&out.stderr)),
        );
    }
    HostError::new(
        HostErrorCode::Other,
        format!("herdr exited {}: {}", out.exit_code, text(&out.stderr)),
    )
}

#[cfg(test)]
mod tests;
