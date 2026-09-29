//! OTLP over HTTP with protobuf bodies: the request messages built from
//! observations and spans, and the blocking POST. Header values are used
//! only in the request and never written to a log or an error (SEC-1).

use std::collections::BTreeMap;

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value};
use opentelemetry_proto::tonic::metrics::v1::{
    AggregationTemporality, Gauge, Histogram, HistogramDataPoint, Metric, NumberDataPoint,
    ResourceMetrics, ScopeMetrics, Sum, metric, number_data_point,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{
    ResourceSpans, ScopeSpans, Span as ProtoSpan, Status, span::SpanKind, status::StatusCode,
};
use prost::Message;

use crate::defs::{Def, Kind};
use crate::observe::Observation;
use crate::spans::{Attr, Span};
use crate::{ExportError, OtlpConfig};

/// Metric points are delta points over buckets of this many milliseconds of
/// event time (ADR 0005 point 4).
pub(crate) const BUCKET_MS: i64 = 60_000;

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        }),
        ..KeyValue::default()
    }
}

fn kv_int(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
        ..KeyValue::default()
    }
}

fn resource(team: &str) -> Resource {
    Resource {
        attributes: vec![
            kv("service.name", "teamwire"),
            kv("service.version", env!("CARGO_PKG_VERSION")),
            kv("teamwire.team", team),
        ],
        ..Resource::default()
    }
}

fn scope() -> InstrumentationScope {
    InstrumentationScope {
        name: "teamwire".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        ..InstrumentationScope::default()
    }
}

fn nanos(millis: i64) -> u64 {
    u64::try_from(millis).unwrap_or(0).saturating_mul(1_000_000)
}

/// What one bucket of one metric with one label set adds up to.
struct Acc {
    def: &'static Def,
    count: u64,
    sum: f64,
    min: f64,
    max: f64,
    bounds: Vec<u64>,
    last: f64,
    time_ms: i64,
}

/// Groups `observations` into delta points and builds the request. The count
/// of points is returned too.
pub(crate) fn metrics_request(
    team: &str,
    observations: &[Observation],
) -> (ExportMetricsServiceRequest, u64) {
    type Key<'a> = (&'static str, &'a [(String, String)], i64);
    let mut table: BTreeMap<Key<'_>, Acc> = BTreeMap::new();
    for o in observations {
        let ms = o.time.unix_millis();
        let bucket = if o.def.kind == Kind::Gauge {
            ms
        } else {
            ms.div_euclid(BUCKET_MS) * BUCKET_MS
        };
        let acc = table
            .entry((o.def.name, o.labels.as_slice(), bucket))
            .or_insert_with(|| Acc {
                def: o.def,
                count: 0,
                sum: 0.0,
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
                bounds: vec![0; o.def.bounds.len() + 1],
                last: 0.0,
                time_ms: bucket,
            });
        acc.count += 1;
        acc.sum += o.value;
        acc.min = acc.min.min(o.value);
        acc.max = acc.max.max(o.value);
        acc.last = o.value;
        if o.def.kind == Kind::Histogram {
            let slot = o
                .def
                .bounds
                .iter()
                .position(|b| o.value <= *b)
                .unwrap_or(o.def.bounds.len());
            if let Some(count) = acc.bounds.get_mut(slot) {
                *count += 1;
            }
        }
    }

    let mut by_metric: BTreeMap<&'static str, Metric> = BTreeMap::new();
    let mut points = 0_u64;
    for ((name, labels, _), acc) in table {
        let attributes: Vec<KeyValue> = labels.iter().map(|(k, v)| kv(k, v)).collect();
        let (start, end) = if acc.def.kind == Kind::Gauge {
            (0, nanos(acc.time_ms))
        } else {
            (nanos(acc.time_ms), nanos(acc.time_ms + BUCKET_MS))
        };
        let metric = by_metric.entry(name).or_insert_with(|| Metric {
            name: name.to_owned(),
            unit: acc.def.unit.to_owned(),
            ..Metric::default()
        });
        points += 1;
        let number = |value: f64| NumberDataPoint {
            attributes: attributes.clone(),
            start_time_unix_nano: start,
            time_unix_nano: end,
            value: Some(number_data_point::Value::AsDouble(value)),
            ..NumberDataPoint::default()
        };
        match acc.def.kind {
            Kind::Counter => {
                let data = metric.data.get_or_insert_with(|| {
                    metric::Data::Sum(Sum {
                        aggregation_temporality: AggregationTemporality::Delta as i32,
                        is_monotonic: true,
                        data_points: Vec::new(),
                    })
                });
                if let metric::Data::Sum(sum) = data {
                    sum.data_points.push(number(acc.sum));
                }
            }
            Kind::Gauge => {
                let data = metric.data.get_or_insert_with(|| {
                    metric::Data::Gauge(Gauge {
                        data_points: Vec::new(),
                    })
                });
                if let metric::Data::Gauge(gauge) = data {
                    gauge.data_points.push(number(acc.last));
                }
            }
            Kind::Histogram => {
                let data = metric.data.get_or_insert_with(|| {
                    metric::Data::Histogram(Histogram {
                        aggregation_temporality: AggregationTemporality::Delta as i32,
                        data_points: Vec::new(),
                    })
                });
                if let metric::Data::Histogram(histogram) = data {
                    histogram.data_points.push(HistogramDataPoint {
                        attributes,
                        start_time_unix_nano: start,
                        time_unix_nano: end,
                        count: acc.count,
                        sum: Some(acc.sum),
                        bucket_counts: acc.bounds,
                        explicit_bounds: acc.def.bounds.to_vec(),
                        min: Some(acc.min),
                        max: Some(acc.max),
                        ..HistogramDataPoint::default()
                    });
                }
            }
        }
    }
    let request = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(resource(team)),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope()),
                metrics: by_metric.into_values().collect(),
                ..ScopeMetrics::default()
            }],
            ..ResourceMetrics::default()
        }],
    };
    (request, points)
}

pub(crate) fn traces_request(
    team: &str,
    trace_id: [u8; 16],
    spans: &[Span],
) -> ExportTraceServiceRequest {
    let spans = spans
        .iter()
        .map(|span| ProtoSpan {
            trace_id: trace_id.to_vec(),
            span_id: span.id.to_vec(),
            parent_span_id: span.parent.map(|p| p.to_vec()).unwrap_or_default(),
            name: span.name.to_owned(),
            kind: SpanKind::Internal as i32,
            start_time_unix_nano: nanos(span.start.unix_millis()),
            end_time_unix_nano: nanos(span.end.unix_millis()),
            attributes: span
                .attrs
                .iter()
                .map(|(key, value)| match value {
                    Attr::Str(text) => kv(key, text),
                    Attr::Int(n) => kv_int(key, *n),
                })
                .collect(),
            status: span.error.as_ref().map(|message| Status {
                code: StatusCode::Error as i32,
                message: message.clone(),
            }),
            ..ProtoSpan::default()
        })
        .collect();
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(resource(team)),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans,
                ..ScopeSpans::default()
            }],
            ..ResourceSpans::default()
        }],
    }
}

/// Posts `message` to `<endpoint>/<path>` with the configured headers. The
/// whole call, connect to last byte, is bounded by the configured timeout.
pub(crate) fn post(
    config: &OtlpConfig,
    path: &str,
    message: &impl Message,
) -> Result<(), ExportError> {
    let url = format!("{}/{path}", config.endpoint.trim_end_matches('/'));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent
        .post(&url)
        .header("Content-Type", "application/x-protobuf");
    for (name, value) in &config.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let response = request
        .send(&message.encode_to_vec()[..])
        .map_err(|e| ExportError::Http(redact(&config.endpoint, &format!("{url}: {e}"))))?;
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(ExportError::Http(redact(
            &config.endpoint,
            &format!("{url}: status {status}"),
        )))
    }
}

/// `text` with the `user:password@` part of `endpoint` (if it has one)
/// replaced, so a credential in the URL never reaches an error message.
fn redact(endpoint: &str, text: &str) -> String {
    let userinfo = endpoint
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest))
        .and_then(|authority| authority.rsplit_once('@'))
        .map(|(userinfo, _)| userinfo);
    match userinfo {
        Some(secret) if !secret.is_empty() => text.replace(secret, "***"),
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn a_credential_in_the_url_is_hidden() {
        let text = "http://bob:hunter2@c:4318/v1/traces: status 401";
        assert_eq!(
            redact("http://bob:hunter2@c:4318", text),
            "http://***@c:4318/v1/traces: status 401"
        );
        assert_eq!(
            redact("http://c:4318", "http://c:4318/x"),
            "http://c:4318/x"
        );
    }
}
