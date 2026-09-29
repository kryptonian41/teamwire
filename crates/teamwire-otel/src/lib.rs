//! Local metrics computed from the event rows (`docs/metrics.md`) and the
//! OpenTelemetry export (OTLP over HTTP): claim, send, commit (ADR 0005).
//!
//! Never: put a message id, session id or path in a metric label; send a
//! token or a secret header value anywhere but the endpoint's headers;
//! hold a store transaction during the HTTP call.
//!
//! Invariants: an event is exported at most once per successful commit of
//! the export mark; trace and span ids are derived from the team and event
//! ids, so a resend produces the same ids.

mod defs;
mod history;
mod local;
mod observe;

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use teamwire_core::TeamName;
use teamwire_store::{Ctx, StoreError};
use thiserror::Error;

const NOT_BUILT: &str = "teamwire-otel is not built yet";

/// Which events `metrics` reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricsQuery {
    /// One team, or every team and archive when `None`.
    pub team: Option<TeamName>,
    /// Only events newer than this.
    pub since: Option<Duration>,
}

/// One metric value with its labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricValue {
    pub name: String,
    pub unit: String,
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

/// `teamwire metrics`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricsView {
    pub metrics: Vec<MetricValue>,
    /// Why part of the view could not be read.
    pub errors: Vec<String>,
}

/// Where the export goes. Header values often hold an API key, so `Debug`
/// prints only the header names.
#[derive(Clone, PartialEq, Eq)]
pub struct OtlpConfig {
    pub endpoint: String,
    pub headers: Vec<(String, String)>,
    pub timeout: Duration,
}

impl fmt::Debug for OtlpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("OtlpConfig")
            .field("endpoint", &self.endpoint)
            .field("header_names", &names)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// What one export sent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportReport {
    /// Events after `from_seq` up to and including `to_seq` were sent.
    pub from_seq: u64,
    pub to_seq: u64,
    pub metric_points: u64,
    pub spans: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExportError {
    /// Another exporter holds a live claim.
    #[error("another exporter holds the claim: {0}")]
    Claimed(String),
    #[error("the endpoint refused or failed: {0}")]
    Http(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

pub use local::metrics_at;

/// The metrics for `q`. With a team named, `ctx.store` is that team's
/// database; without one, every team and archive under `ctx.home` is read.
pub fn metrics(ctx: &Ctx<'_>, q: MetricsQuery) -> MetricsView {
    local::metrics_with(ctx.home, ctx.store, ctx.clock.now(), &q)
}

/// Sends the events after the export mark, then moves the mark.
pub fn export_once(
    _ctx: &mut Ctx<'_>,
    _exporter: &OtlpConfig,
) -> Result<ExportReport, ExportError> {
    Err(ExportError::Internal(NOT_BUILT.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_header_values() {
        let config = OtlpConfig {
            endpoint: "http://localhost:4318".to_owned(),
            headers: vec![("authorization".to_owned(), "Bearer sk-123".to_owned())],
            timeout: Duration::from_secs(5),
        };
        let text = format!("{config:?}");
        assert!(text.contains("authorization"));
        assert!(!text.contains("sk-123"));
    }
}
