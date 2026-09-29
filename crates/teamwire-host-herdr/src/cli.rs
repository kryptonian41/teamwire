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
    let cmd = Command::new("herdr").args(args.iter().cloned());
    let out = platform.run(&cmd, timeout).map_err(|e| {
        HostError::new(
            HostErrorCode::Other,
            format!("could not run herdr: {}", clip(&e.to_string())),
        )
    })?;
    interpret(&out)
}

fn interpret(out: &RunOutput) -> Result<Value, HostError> {
    if out.timed_out {
        return Err(HostError::new(
            HostErrorCode::Timeout,
            "herdr did not answer in time",
        ));
    }
    let parsed = envelope(&out.stdout).or_else(|| envelope(&out.stderr));
    if let Some(body) = parsed.as_ref().and_then(|e| e.error.as_ref()) {
        return Err(from_body(body));
    }
    if out.success() {
        return parsed.and_then(|e| e.result).ok_or_else(|| {
            HostError::new(
                HostErrorCode::Other,
                format!("herdr printed no result: {}", text(&out.stdout)),
            )
        });
    }
    if out.exit_code == 2 {
        return Err(HostError::new(
            HostErrorCode::BadArgument,
            format!("herdr usage error: {}", text(&out.stderr)),
        ));
    }
    Err(HostError::new(
        HostErrorCode::Other,
        format!("herdr exited {}: {}", out.exit_code, text(&out.stderr)),
    ))
}

#[cfg(test)]
mod tests;
