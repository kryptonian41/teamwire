//! `teamwire metrics`: local metrics, and `metrics export` to OpenTelemetry.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, MetricsArgs};

pub(crate) fn run(args: &MetricsArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let MetricsArgs {
        team: _team,
        since: _since,
        json: _json,
        action: _action,
    } = args;
    crate::not_built("metrics", global, wiring, false)
}
