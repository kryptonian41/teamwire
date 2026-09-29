#![expect(
    clippy::indexing_slicing,
    reason = "a test asserts the length of the list it then indexes; an out-of-range index fails the test"
)]

use serde_json::{Value, json};
use teamwire_core::{Event, EventType, Role, Timestamp, Ulid};

use super::*;

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

/// A log built in order: `seq` counts up from 1.
struct Log(Vec<Event>);

impl Log {
    fn add(
        &mut self,
        secs: i64,
        kind: EventType,
        who: Option<&str>,
        subject: Option<Ulid>,
        data: Value,
    ) -> Event {
        let event = Event {
            seq: self.0.len() as u64 + 1,
            ulid: Ulid::from_parts(at(secs), u128::from(self.0.len() as u64 + 1)),
            time: at(secs),
            kind,
            role: who.map(|w| Role::parse(w).unwrap()),
            subject,
            data,
            writer_version: "test".to_owned(),
        };
        self.0.push(event.clone());
        event
    }

    fn spans(&self, end: &Event) -> Vec<Span> {
        spans_ending(&History::new(self.0.clone()), end, "alpha", "TEAMULID")
    }
}

fn attr<'a>(span: &'a Span, key: &str) -> Option<&'a Attr> {
    span.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

#[test]
fn ids_are_the_same_every_run_and_differ_by_kind() {
    assert_eq!(trace_id("T1"), trace_id("T1"));
    assert_ne!(trace_id("T1"), trace_id("T2"));
    assert_eq!(span_id("message", "M"), span_id("message", "M"));
    assert_ne!(span_id("message", "M"), span_id("delivery", "M"));
}

#[test]
fn a_generation_span_carries_the_harness_session_id() {
    let mut log = Log(Vec::new());
    log.add(
        1,
        EventType::MemberStarted,
        Some("api"),
        None,
        json!({"session_id": "sess-9"}),
    );
    let done = log.add(
        31,
        EventType::MemberCompleted,
        Some("api"),
        Some(Ulid::from_parts(at(31), 1)),
        json!({"status": "done", "generation": 1}),
    );
    let spans = log.spans(&done);
    let span = spans
        .iter()
        .find(|s| s.name == "teamwire.member.generation")
        .unwrap();
    assert_eq!(
        attr(span, "session.id"),
        Some(&Attr::Str("sess-9".to_owned()))
    );
    assert_eq!(attr(span, "teamwire.generation"), Some(&Attr::Int(1)));
    assert_eq!(span.end.saturating_since(span.start).as_secs(), 30);
}

#[test]
fn a_launch_span_ends_at_the_event_that_settles_the_last_member() {
    let mut log = Log(Vec::new());
    let data = |extra: Value| {
        let mut d = json!({"launch": "L1", "requested": 2});
        d.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        d
    };
    log.add(
        1,
        EventType::MemberCreating,
        Some("api"),
        None,
        data(json!({"kind": "implementer"})),
    );
    log.add(
        2,
        EventType::MemberCreating,
        Some("web"),
        None,
        data(json!({"kind": "implementer"})),
    );
    let first = log.add(
        5,
        EventType::MemberStarted,
        Some("api"),
        None,
        data(json!({})),
    );
    let names = |spans: &[Span]| spans.iter().map(|s| s.name).collect::<Vec<_>>();
    assert_eq!(names(&log.spans(&first)), ["teamwire.launch.member"]);
    let second = log.add(
        9,
        EventType::MemberFailed,
        Some("web"),
        None,
        data(json!({"reason": "no_place"})),
    );
    let spans = log.spans(&second);
    assert_eq!(names(&spans), ["teamwire.launch.member", "teamwire.launch"]);
    let launch = &spans[1];
    assert_eq!(launch.end.saturating_since(launch.start).as_secs(), 8);
    assert_eq!(
        attr(launch, "teamwire.members.started"),
        Some(&Attr::Int(1))
    );
    // The failed member's span says why, and hangs under the launch.
    assert_eq!(spans[0].error.as_deref(), Some("no_place"));
    assert_eq!(spans[0].parent, Some(span_id("launch", "L1")));
}

#[test]
fn only_a_completed_close_ends_the_team_span() {
    let mut log = Log(Vec::new());
    log.add(
        0,
        EventType::TeamCreated,
        None,
        None,
        json!({"harness": "claude-code", "host": "herdr"}),
    );
    log.add(50, EventType::TeamClosing, None, None, json!({}));
    let failed = log.add(
        60,
        EventType::TeamClosed,
        None,
        None,
        json!({"outcome": "failed", "failed_step": "archive"}),
    );
    let names = |spans: &[Span]| spans.iter().map(|s| s.name).collect::<Vec<_>>();
    let spans = log.spans(&failed);
    assert_eq!(names(&spans), ["teamwire.close"]);
    assert_eq!(spans[0].error.as_deref(), Some("archive"));
    log.add(70, EventType::TeamClosing, None, None, json!({}));
    let done = log.add(
        80,
        EventType::TeamClosed,
        None,
        None,
        json!({"outcome": "completed"}),
    );
    let spans = log.spans(&done);
    assert_eq!(names(&spans), ["teamwire.close", "teamwire.team"]);
    assert_eq!(spans[1].parent, None);
    assert_eq!(spans[1].id, span_id("team", "TEAMULID"));
    assert_eq!(
        attr(&spans[1], "teamwire.host"),
        Some(&Attr::Str("herdr".to_owned()))
    );
    // The close span starts at the closing that this end belongs to.
    assert_eq!(spans[0].end.saturating_since(spans[0].start).as_secs(), 10);
}

#[test]
fn a_message_span_waits_for_every_recipient_and_a_delivery_span_is_sent_once() {
    let mut log = Log(Vec::new());
    let m = Ulid::from_parts(at(10), 99);
    log.add(
        10,
        EventType::MessageSent,
        Some("api"),
        Some(m),
        json!({"type": "status", "recipients": ["lead", "web"]}),
    );
    let first = log.add(
        12,
        EventType::DeliveryHandedOff,
        None,
        Some(m),
        json!({"route": "host", "attempts": 1}),
    );
    let names = |spans: &[Span]| spans.iter().map(|s| s.name).collect::<Vec<_>>();
    assert_eq!(names(&log.spans(&first)), ["teamwire.delivery"]);
    let second = log.add(
        15,
        EventType::DeliveryUndelivered,
        Some("web"),
        Some(m),
        json!({"route": "host", "attempts": 3, "code": "host_busy"}),
    );
    let spans = log.spans(&second);
    assert_eq!(names(&spans), ["teamwire.delivery", "teamwire.message"]);
    assert_eq!(spans[0].error.as_deref(), Some("host_busy"));
    assert_eq!(spans[0].parent, Some(spans[1].id));
    // A later outcome for a recipient that already has one adds no span.
    let again = log.add(
        20,
        EventType::DeliveryUnconfirmed,
        Some("web"),
        Some(m),
        json!({"route": "host"}),
    );
    assert!(log.spans(&again).is_empty());
}

#[test]
fn a_result_span_starts_at_the_completion_and_hangs_under_its_generation() {
    let mut log = Log(Vec::new());
    let started = log.add(1, EventType::MemberStarted, Some("api"), None, json!({}));
    let c = Ulid::from_parts(at(20), 5);
    log.add(
        20,
        EventType::MemberCompleted,
        Some("api"),
        Some(c),
        json!({"status": "done", "generation": 1}),
    );
    let delivered = log.add(
        24,
        EventType::ResultDelivered,
        Some("api"),
        Some(c),
        json!({"route": "host"}),
    );
    let spans = log.spans(&delivered);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].end.saturating_since(spans[0].start).as_secs(), 4);
    assert_eq!(
        spans[0].parent,
        Some(span_id("generation", &started.ulid.to_string()))
    );
}

#[test]
fn a_verify_span_starts_when_the_check_started() {
    let mut log = Log(Vec::new());
    let recorded = log.add(
        100,
        EventType::VerifyRecorded,
        Some("api"),
        None,
        json!({"verdict": "PASS", "seconds": 12}),
    );
    let spans = log.spans(&recorded);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].end.saturating_since(spans[0].start).as_secs(), 12);
    assert_eq!(
        attr(&spans[0], "teamwire.verdict"),
        Some(&Attr::Str("PASS".to_owned()))
    );
}
