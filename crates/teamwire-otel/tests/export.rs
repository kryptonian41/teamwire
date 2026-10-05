//! `export_once` against an in-process OTLP receiver.
#![expect(
    clippy::disallowed_methods,
    reason = "one test times a call to a receiver that never answers"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "helper functions in an integration-test file unwrap, slice and panic; a failure there fails the test"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]

#[path = "support/stub.rs"]
mod stub;

use std::sync::{Arc, Barrier};
use std::time::Duration;

use opentelemetry_proto::tonic::metrics::v1::metric::Data;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use stub::{Mode, Stub, closed_port_url};
use teamwire_core::{
    Clock, Config, EventType, HarnessId, HostId, LeadRoute, Pid, Registry, Role, TeamName,
    Timestamp, Ulid,
};
use teamwire_fakes::{FakeClock, FakePlatform, TestHome};
use teamwire_otel::{ExportError, OtlpConfig, export_once};
use teamwire_store::{Ctx, NewTeam, Store};

const T0: i64 = 1_790_000_000_000;

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(T0 + secs * 1000).unwrap()
}

struct Env {
    home: TestHome,
    store: Store,
    registry: Registry,
    platform: FakePlatform,
    clock: FakeClock,
    config: Config,
    team_ulid: Ulid,
}

fn env() -> Env {
    let home = TestHome::new().unwrap();
    let name = TeamName::parse("alpha").unwrap();
    let mut store = Store::open(home.path(), &name).unwrap();
    let team_ulid = store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: name.clone(),
                    title: "Alpha".to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )
        })
        .unwrap();
    Env {
        home,
        store,
        registry: Registry::new(),
        platform: FakePlatform::new(),
        clock: FakeClock::starting_at(at(7200)),
        config: Config::default(),
        team_ulid,
    }
}

impl Env {
    fn ev(
        &mut self,
        secs: i64,
        kind: EventType,
        who: Option<&str>,
        subject: Option<&Ulid>,
        data: &Value,
    ) -> Ulid {
        let who = who.map(|w| Role::parse(w).unwrap());
        self.store
            .write(|tx| tx.record_event(at(secs), kind, who.as_ref(), subject, data))
            .unwrap()
    }

    fn message(&mut self, secs: i64) -> Ulid {
        let m = Ulid::from_parts(at(secs), 1);
        self.ev(
            secs,
            EventType::MessageSent,
            Some("api"),
            Some(&m),
            &json!({"type": "status", "sender_kind": "member", "recipients": ["lead"]}),
        );
        m
    }

    fn handed_off(&mut self, secs: i64, m: &Ulid) {
        self.ev(
            secs,
            EventType::DeliveryHandedOff,
            None,
            Some(m),
            &json!({"route": "host", "attempts": 1, "seconds": 1}),
        );
    }

    fn export(&mut self, url: &str) -> Result<teamwire_otel::ExportReport, ExportError> {
        let mut ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        export_once(&mut ctx, &config(url))
    }

    fn mark(&self) -> teamwire_store::ExportMark {
        self.store.read(|tx| tx.export_mark()).unwrap()
    }

    fn last_seq(&self) -> u64 {
        self.store.read(|tx| tx.last_event_seq()).unwrap()
    }
}

fn config(url: &str) -> OtlpConfig {
    OtlpConfig {
        endpoint: url.to_owned(),
        headers: Vec::new(),
        timeout: Duration::from_secs(5),
    }
}

fn spans_of(stub: &Stub) -> Vec<opentelemetry_proto::tonic::trace::v1::Span> {
    stub.traces()
        .into_iter()
        .flat_map(|r| r.resource_spans)
        .flat_map(|r| r.scope_spans)
        .flat_map(|s| s.spans)
        .collect()
}

#[test]
fn collector_down_keeps_mark() {
    let mut e = env();
    let m = e.message(10);
    e.handed_off(11, &m);
    let err = e.export(&closed_port_url()).unwrap_err();
    assert!(matches!(err, ExportError::Http(_)), "{err:?}");
    let mark = e.mark();
    assert_eq!(mark.last_event_seq, 0, "the mark moved");
    assert_eq!(mark.claimed_to, None, "the claim was left behind");
    // The next run, with the collector back, sends everything once.
    let stub = Stub::start(Mode::Ok);
    let report = e.export(&stub.url()).unwrap();
    assert_eq!((report.from_seq, report.to_seq), (0, e.last_seq()));
    assert_eq!(e.mark().last_event_seq, e.last_seq());
    assert_eq!(stub.metrics().len(), 1);
}

#[test]
fn crash_before_mark_reexports_once() {
    let mut e = env();
    let m = e.message(10);
    e.handed_off(11, &m);
    // An exporter that died after sending, before it moved the mark.
    let dead = "999999";
    let claimed_at = e.clock.now();
    e.store
        .write(|tx| tx.claim_metric_export(claimed_at, dead, Duration::from_secs(300), |_| false))
        .unwrap()
        .unwrap();
    let stub = Stub::start(Mode::Ok);
    // Too young to take over.
    assert!(matches!(
        e.export(&stub.url()),
        Err(ExportError::Claimed(_))
    ));
    assert!(stub.requests().is_empty());
    e.clock.advance(Duration::from_secs(301));
    let report = e.export(&stub.url()).unwrap();
    assert_eq!(report.to_seq, e.last_seq());
    assert_eq!(stub.metrics().len(), 1, "the batch is sent once more");
    // And only once: the mark moved, so a third run has nothing to send.
    let again = e.export(&stub.url()).unwrap();
    assert_eq!((again.metric_points, again.spans), (0, 0));
    assert_eq!(stub.metrics().len(), 1);
}

#[test]
fn two_exporters_never_send_one_batch_twice() {
    let mut e = env();
    let m = e.message(10);
    e.handed_off(11, &m);
    let stub = Stub::start(Mode::Ok);

    // A live exporter's claim: this one sends nothing.
    let holder = Pid::new(4242);
    e.platform.set_alive(holder, true);
    let at_now = e.clock.now();
    e.store
        .write(|tx| tx.claim_metric_export(at_now, "4242", Duration::from_secs(300), |_| false))
        .unwrap()
        .unwrap();
    e.clock.advance(Duration::from_secs(400));
    assert!(matches!(
        e.export(&stub.url()),
        Err(ExportError::Claimed(_))
    ));
    assert!(stub.requests().is_empty());
    e.store.write(|tx| tx.release_metric_claim("4242")).unwrap();

    // Four exporters at once, each with its own connection to one database.
    let name = TeamName::parse("alpha").unwrap();
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (home, url, barrier) = (
                e.home.path().to_path_buf(),
                stub.url(),
                Arc::clone(&barrier),
            );
            let name = name.clone();
            let clock = e.clock.clone();
            std::thread::spawn(move || {
                let mut store = Store::open_existing(&home, &name).unwrap();
                let (registry, platform, config) =
                    (Registry::new(), FakePlatform::new(), Config::default());
                barrier.wait();
                let mut ctx = Ctx {
                    home: &home,
                    store: &mut store,
                    registry: &registry,
                    platform: &platform,
                    clock: &clock,
                    config: &config,
                };
                export_once(
                    &mut ctx,
                    &OtlpConfig {
                        endpoint: url,
                        headers: Vec::new(),
                        timeout: Duration::from_secs(5),
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(results.iter().any(Result::is_ok), "{results:?}");
    assert_eq!(stub.metrics().len(), 1, "one batch, sent once: {results:?}");
    // No counter doubled: the one message was counted once.
    let sent: f64 = stub
        .metrics()
        .iter()
        .flat_map(|r| &r.resource_metrics)
        .flat_map(|r| &r.scope_metrics)
        .flat_map(|s| &s.metrics)
        .filter(|m| m.name == "teamwire.messages.sent")
        .filter_map(|m| match &m.data {
            Some(Data::Sum(s)) => Some(s.data_points.clone()),
            _ => None,
        })
        .flatten()
        .map(|p| match p.value {
            Some(opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsDouble(
                v,
            )) => v,
            _ => 0.0,
        })
        .sum();
    assert!((sent - 1.0).abs() < f64::EPSILON, "{sent}");
}

#[test]
fn span_across_batches_is_whole() {
    let mut e = env();
    let stub = Stub::start(Mode::Ok);
    let m = e.message(10);
    e.export(&stub.url()).unwrap();
    assert!(
        spans_of(&stub).iter().all(|s| s.name != "teamwire.message"),
        "a span whose end has not happened is not sent"
    );
    e.handed_off(70, &m);
    e.export(&stub.url()).unwrap();
    let spans = spans_of(&stub);
    let message: Vec<_> = spans
        .iter()
        .filter(|s| s.name == "teamwire.message")
        .collect();
    assert_eq!(message.len(), 1, "sent once");
    let span = message[0];
    // The start is the first batch's event, the end the second batch's.
    assert_eq!(
        span.start_time_unix_nano,
        u64::try_from(T0 + 10_000).unwrap() * 1_000_000
    );
    assert_eq!(
        span.end_time_unix_nano,
        u64::try_from(T0 + 70_000).unwrap() * 1_000_000
    );
    let trace = &Sha256::digest(e.team_ulid.to_string().as_bytes())[..16];
    assert_eq!(span.trace_id, trace);
    let delivery = spans
        .iter()
        .find(|s| s.name == "teamwire.delivery")
        .unwrap();
    assert_eq!(
        delivery.parent_span_id, span.span_id,
        "the child names its parent"
    );
    assert_eq!(delivery.trace_id, span.trace_id);
}

#[test]
fn backlog_keeps_event_times_as_deltas() {
    let mut e = env();
    // Two messages an hour apart, exported together long after.
    e.message(10);
    e.message(3610);
    let stub = Stub::start(Mode::Ok);
    e.export(&stub.url()).unwrap();
    let requests = stub.metrics();
    let metric = requests
        .iter()
        .flat_map(|r| &r.resource_metrics)
        .flat_map(|r| &r.scope_metrics)
        .flat_map(|s| &s.metrics)
        .find(|m| m.name == "teamwire.messages.sent")
        .unwrap();
    let Some(Data::Sum(sum)) = &metric.data else {
        panic!("a counter is a sum: {metric:?}");
    };
    // 1 is DELTA in the OTLP enum (2 is CUMULATIVE).
    assert_eq!(sum.aggregation_temporality, 1, "delta");
    assert!(sum.is_monotonic);
    assert_eq!(sum.data_points.len(), 2);
    let minute = 60_000_000_000_u64;
    let bucket = |secs: i64| u64::try_from(T0 + secs * 1000).unwrap() * 1_000_000 / minute * minute;
    let mut times: Vec<(u64, u64)> = sum
        .data_points
        .iter()
        .map(|p| (p.start_time_unix_nano, p.time_unix_nano))
        .collect();
    times.sort_unstable();
    assert_eq!(times[0], (bucket(10), bucket(10) + minute));
    assert_eq!(times[1], (bucket(3610), bucket(3610) + minute));
    // The export time (two hours in) appears nowhere.
    let export_time = u64::try_from(T0 + 7200 * 1000).unwrap() * 1_000_000;
    assert!(times.iter().all(|(_, end)| *end < export_time));
}

#[test]
fn a_resend_carries_the_same_trace_and_span_ids() {
    let mut e = env();
    let m = e.message(10);
    e.handed_off(20, &m);
    let refusing = Stub::start(Mode::Status(503));
    assert!(matches!(
        e.export(&refusing.url()),
        Err(ExportError::Http(_))
    ));
    let good = Stub::start(Mode::Ok);
    e.export(&good.url()).unwrap();
    let ids = |s: &Stub| {
        let mut v: Vec<_> = spans_of(s)
            .into_iter()
            .map(|s| (s.name, s.trace_id, s.span_id, s.parent_span_id))
            .collect();
        v.sort();
        v
    };
    assert!(!ids(&good).is_empty());
    assert_eq!(ids(&refusing), ids(&good));
}

#[test]
fn headers_reach_the_collector_and_never_the_error_text() {
    let mut e = env();
    e.message(10);
    let stub = Stub::start(Mode::Status(401));
    let mut cfg = config(&stub.url());
    cfg.headers = vec![(
        "authorization".to_owned(),
        "Bearer sk-very-secret".to_owned(),
    )];
    let err = {
        let mut ctx = Ctx {
            home: e.home.path(),
            store: &mut e.store,
            registry: &e.registry,
            platform: &e.platform,
            clock: &e.clock,
            config: &e.config,
        };
        export_once(&mut ctx, &cfg).unwrap_err()
    };
    let sent = stub.requests();
    assert!(!sent.is_empty());
    assert!(sent.iter().all(|r| r.headers.contains(&(
        "authorization".to_owned(),
        "Bearer sk-very-secret".to_owned()
    ))));
    assert!(!format!("{err} {err:?} {cfg:?}").contains("sk-very-secret"));
}

#[test]
fn a_collector_that_never_answers_is_cut_off_at_the_timeout() {
    let mut e = env();
    e.message(10);
    let stub = Stub::start(Mode::Delay(Duration::from_secs(20)));
    let mut cfg = config(&stub.url());
    cfg.timeout = Duration::from_millis(300);
    let started = std::time::Instant::now();
    let err = {
        let mut ctx = Ctx {
            home: e.home.path(),
            store: &mut e.store,
            registry: &e.registry,
            platform: &e.platform,
            clock: &e.clock,
            config: &e.config,
        };
        export_once(&mut ctx, &cfg).unwrap_err()
    };
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(matches!(err, ExportError::Http(_)), "{err:?}");
    assert_eq!(e.mark().last_event_seq, 0);
    assert_eq!(e.mark().claimed_to, None);
}

#[test]
fn nothing_new_sends_nothing() {
    let mut e = env();
    let stub = Stub::start(Mode::Ok);
    e.export(&stub.url()).unwrap();
    let before = stub.requests().len();
    let report = e.export(&stub.url()).unwrap();
    assert_eq!((report.metric_points, report.spans), (0, 0));
    assert_eq!(stub.requests().len(), before);
}
