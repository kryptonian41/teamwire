//! Spans rebuilt from event pairs (`docs/metrics.md` section 3, ADR 0005
//! point 5). A span is produced when its end event is in the batch. Its
//! start is found by id in the whole history, so a span whose start went out
//! in an earlier batch is still whole. Ids are derived from event ULIDs, so
//! a resend produces the same ids.

use serde_json::Value;
use sha2::{Digest, Sha256};
use teamwire_core::{Event, EventType, Role, Timestamp};

use crate::history::{History, number, text};

pub(crate) type SpanId = [u8; 8];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Attr {
    Str(String),
    Int(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    pub(crate) name: &'static str,
    pub(crate) id: SpanId,
    pub(crate) parent: Option<SpanId>,
    pub(crate) start: Timestamp,
    pub(crate) end: Timestamp,
    pub(crate) attrs: Vec<(&'static str, Attr)>,
    /// Set when the span ended in a failure.
    pub(crate) error: Option<String>,
}

/// The first 16 bytes of the SHA-256 of the team's ULID.
pub(crate) fn trace_id(team_ulid: &str) -> [u8; 16] {
    let digest = Sha256::digest(team_ulid.as_bytes());
    digest.first_chunk().copied().unwrap_or_default()
}

/// The first 8 bytes of the SHA-256 of `kind:key`. `key` is an event's ULID
/// (or a launch id), and `kind` keeps two spans that start at one event apart.
pub(crate) fn span_id(kind: &str, key: &str) -> SpanId {
    let digest = Sha256::digest(format!("{kind}:{key}").as_bytes());
    digest.first_chunk().copied().unwrap_or_default()
}

/// A count stored as a JSON number; zero when absent.
#[expect(
    clippy::cast_possible_truncation,
    reason = "generations, attempts and milliseconds of a check are far below 2^53"
)]
fn whole(v: Option<f64>) -> i64 {
    v.map_or(0, |f| f as i64)
}

fn s(v: impl Into<String>) -> Attr {
    Attr::Str(v.into())
}

/// The spans whose end event is `e`.
pub(crate) fn spans_ending(h: &History, e: &Event, team: &str, team_ulid: &str) -> Vec<Span> {
    let team_span = span_id("team", team_ulid);
    match e.kind {
        EventType::TeamClosed => team_closed(h, e, team, team_span),
        EventType::MemberStarted | EventType::MemberFailed | EventType::MemberRefused => {
            launch(h, e, team_span)
        }
        EventType::MemberCompleted => generation(h, e, team_span).into_iter().collect(),
        EventType::DeliveryHandedOff
        | EventType::DeliveryUnconfirmed
        | EventType::DeliveryUndelivered => delivery(h, e, team_span),
        EventType::ResultDelivered | EventType::ResultUndelivered => {
            result(h, e).into_iter().collect()
        }
        EventType::VerifyRecorded => verify(e, team_span).into_iter().collect(),
        EventType::TeamCreated
        | EventType::TeamClosing
        | EventType::TeamLeadAdopted
        | EventType::MemberCreating
        | EventType::MemberReplaced
        | EventType::MemberRemoved
        | EventType::MemberReprompted
        | EventType::MemberCompletionRefused
        | EventType::MemberHeld
        | EventType::MessageSent
        | EventType::MessageRefused
        | EventType::DeliveryAcked
        | EventType::DeliveryReminded
        | EventType::DeliveryResent
        | EventType::DeliveryReentered
        | EventType::ResultDelivering
        | EventType::ResultUnconfirmed
        | EventType::WatchStarted
        | EventType::WatchLine
        | EventType::WatchStopped
        | EventType::VerifyWaived
        | EventType::CloseStep
        | EventType::CloseRefused
        | EventType::StoreError => Vec::new(),
    }
}

fn team_closed(h: &History, e: &Event, team: &str, team_span: SpanId) -> Vec<Span> {
    let mut out = Vec::new();
    let before = || h.events().iter().take_while(|p| p.seq < e.seq);
    if let Some(closing) = before().filter(|p| p.kind == EventType::TeamClosing).last() {
        out.push(Span {
            name: "teamwire.close",
            id: span_id("close", &closing.ulid.to_string()),
            parent: Some(team_span),
            start: closing.time,
            end: e.time,
            attrs: vec![(
                "teamwire.failed_step",
                s(text(&e.data, "failed_step").unwrap_or("")),
            )],
            error: text(&e.data, "failed_step").map(str::to_owned),
        });
    }
    // Only a completed close ends the team's own span.
    let created = h.events().iter().find(|p| p.kind == EventType::TeamCreated);
    if let (Some("completed"), Some(created)) = (text(&e.data, "outcome"), created) {
        let field = |key| s(text(&created.data, key).unwrap_or(""));
        out.push(Span {
            name: "teamwire.team",
            id: team_span,
            parent: None,
            start: created.time,
            end: e.time,
            attrs: vec![
                ("teamwire.team", s(team)),
                ("teamwire.harness", field("harness")),
                ("teamwire.host", field("host")),
            ],
            error: None,
        });
    }
    out
}

fn launch(h: &History, e: &Event, team_span: SpanId) -> Vec<Span> {
    let mut out = Vec::new();
    let launch_id = text(&e.data, "launch");
    let parent = launch_id.map_or(team_span, |id| span_id("launch", id));
    let role = e.role.as_ref().map_or("", |r| r.as_str());
    // A member refused before it was recorded has no `member.creating` in
    // this launch, so no member span; an older launch's row is not its start.
    if let Some(creating) = e.role.as_ref().and_then(|r| {
        h.of_role(r.as_str())
            .take_while(|p| p.seq < e.seq)
            .filter(|p| p.kind == EventType::MemberCreating)
            .last()
            .filter(|p| text(&p.data, "launch") == launch_id)
    }) {
        let failed = e.kind != EventType::MemberStarted;
        let reason = text(&e.data, "reason").unwrap_or("failed");
        let mut attrs = vec![
            ("teamwire.role", s(role)),
            (
                "teamwire.kind",
                s(text(&creating.data, "kind").unwrap_or("unknown")),
            ),
        ];
        if failed {
            attrs.push(("error.type", s(reason)));
        }
        out.push(Span {
            name: "teamwire.launch.member",
            id: span_id("launch.member", &creating.ulid.to_string()),
            parent: Some(parent),
            start: creating.time,
            end: e.time,
            attrs,
            error: failed.then(|| reason.to_owned()),
        });
    }
    if let Some(id) = launch_id {
        out.extend(launch_span(h, e, id, team_span));
    }
    out
}

/// The launch span, at the terminal event that makes every requested member
/// started, failed or refused.
fn launch_span(h: &History, e: &Event, id: &str, team_span: SpanId) -> Option<Span> {
    let requested = e.data.get("requested").and_then(Value::as_u64)?;
    let mine = || {
        h.events()
            .iter()
            .take_while(|p| p.seq <= e.seq)
            .filter(|p| text(&p.data, "launch") == Some(id))
    };
    let terminal = |p: &&Event| {
        matches!(
            p.kind,
            EventType::MemberStarted | EventType::MemberFailed | EventType::MemberRefused
        )
    };
    if mine().filter(terminal).count() as u64 != requested {
        return None;
    }
    let started = mine()
        .filter(|p| p.kind == EventType::MemberStarted)
        .count();
    Some(Span {
        name: "teamwire.launch",
        id: span_id("launch", id),
        parent: Some(team_span),
        start: mine().next()?.time,
        end: e.time,
        attrs: vec![
            (
                "teamwire.members.requested",
                Attr::Int(i64::try_from(requested).unwrap_or(i64::MAX)),
            ),
            (
                "teamwire.members.started",
                Attr::Int(i64::try_from(started).unwrap_or(i64::MAX)),
            ),
        ],
        error: None,
    })
}

fn generation_span_id(start: &Event) -> SpanId {
    span_id("generation", &start.ulid.to_string())
}

fn generation(h: &History, e: &Event, team_span: SpanId) -> Option<Span> {
    let start = h.generation_start(e)?;
    let role = e.role.as_ref()?;
    let session = h
        .of_role(role.as_str())
        .take_while(|p| p.seq <= e.seq)
        .filter(|p| p.kind == EventType::MemberStarted)
        .filter_map(|p| text(&p.data, "session_id"))
        .last();
    let mut attrs = vec![
        ("teamwire.role", s(role.as_str())),
        (
            "teamwire.generation",
            Attr::Int(whole(number(&e.data, "generation"))),
        ),
        (
            "teamwire.status",
            s(text(&e.data, "status").unwrap_or("unknown")),
        ),
    ];
    if let Some(session) = session {
        attrs.push(("session.id", s(session)));
    }
    Some(Span {
        name: "teamwire.member.generation",
        id: generation_span_id(start),
        parent: Some(team_span),
        start: start.time,
        end: e.time,
        attrs,
        error: None,
    })
}

/// The recipient a delivery event names. Every delivery event records it as
/// `to`; an event written before that names a member by its role and the
/// lead by no role.
pub(crate) fn recipient_of(e: &Event) -> &str {
    e.data
        .get("to")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| e.role.as_ref().map_or("lead", |r| r.as_str()))
}

fn is_outcome(kind: EventType) -> bool {
    matches!(
        kind,
        EventType::DeliveryHandedOff
            | EventType::DeliveryUnconfirmed
            | EventType::DeliveryUndelivered
    )
}

/// The delivery span of the first outcome of one recipient, and the message
/// span when that outcome is the last one the message was waiting for.
fn delivery(h: &History, e: &Event, team_span: SpanId) -> Vec<Span> {
    let Some(message) = e.subject.as_ref().map(ToString::to_string) else {
        return Vec::new();
    };
    let Some(sent) = h.about(&message).find(|p| p.kind == EventType::MessageSent) else {
        return Vec::new();
    };
    let to = recipient_of(e);
    let earlier = || {
        h.about(&message)
            .take_while(|p| p.seq < e.seq)
            .filter(|p| is_outcome(p.kind))
    };
    if earlier().any(|p| recipient_of(p) == to) {
        return Vec::new();
    }
    let message_span = span_id("message", &message);
    let code = text(&e.data, "code");
    let outcome = if e.kind == EventType::DeliveryHandedOff {
        "handed_off"
    } else if e.kind == EventType::DeliveryUnconfirmed {
        "unconfirmed"
    } else {
        "undelivered"
    };
    let mut attrs = vec![
        ("teamwire.to", s(to)),
        (
            "teamwire.route",
            s(text(&e.data, "route").unwrap_or("none")),
        ),
        ("teamwire.outcome", s(outcome)),
        (
            "teamwire.attempts",
            Attr::Int(whole(number(&e.data, "attempts"))),
        ),
    ];
    if let Some(code) = code {
        attrs.push(("error.type", s(code)));
    }
    let mut out = vec![Span {
        name: "teamwire.delivery",
        id: span_id("delivery", &format!("{message}/{to}")),
        parent: Some(message_span),
        start: sent.time,
        end: e.time,
        attrs,
        error: (outcome == "undelivered").then(|| code.unwrap_or("undelivered").to_owned()),
    }];
    let recipients: Vec<&str> = sent
        .data
        .get("recipients")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let covered = |who: &str| recipient_of(e) == who || earlier().any(|p| recipient_of(p) == who);
    if !recipients.is_empty() && recipients.iter().all(|who| covered(who)) {
        out.push(Span {
            name: "teamwire.message",
            id: message_span,
            parent: Some(team_span),
            start: sent.time,
            end: e.time,
            attrs: vec![
                ("teamwire.message.id", s(message.clone())),
                (
                    "teamwire.message.type",
                    s(text(&sent.data, "type").unwrap_or("unknown")),
                ),
                (
                    "teamwire.from",
                    s(text(&sent.data, "from")
                        .or_else(|| sent.role.as_ref().map(Role::as_str))
                        .unwrap_or("lead")),
                ),
            ],
            error: None,
        });
    }
    out
}

fn result(h: &History, e: &Event) -> Option<Span> {
    let subject = e.subject.as_ref()?.to_string();
    let completed = h
        .about(&subject)
        .find(|p| p.kind == EventType::MemberCompleted)?;
    let parent = h.generation_start(completed).map(generation_span_id);
    let outcome = if e.kind == EventType::ResultDelivered {
        "delivered"
    } else {
        "undelivered"
    };
    Some(Span {
        name: "teamwire.result",
        id: span_id("result", &e.ulid.to_string()),
        parent,
        start: completed.time,
        end: e.time,
        attrs: vec![
            (
                "teamwire.route",
                s(text(&e.data, "route").unwrap_or("none")),
            ),
            ("teamwire.outcome", s(outcome)),
        ],
        error: (outcome == "undelivered")
            .then(|| text(&e.data, "code").unwrap_or("undelivered").to_owned()),
    })
}

fn verify(e: &Event, team_span: SpanId) -> Option<Span> {
    let role = e.role.as_ref()?;
    let millis = whole(number(&e.data, "seconds").map(|secs| secs * 1000.0));
    let start = Timestamp::from_unix_millis(e.time.unix_millis().saturating_sub(millis))?;
    let mut attrs = vec![
        ("teamwire.role", s(role.as_str())),
        (
            "teamwire.verdict",
            s(text(&e.data, "verdict").unwrap_or("unknown")),
        ),
    ];
    if let Some(head) = text(&e.data, "head_sha") {
        attrs.push(("teamwire.head_sha", s(head)));
    }
    Some(Span {
        name: "teamwire.verify.member",
        id: span_id("verify", &e.ulid.to_string()),
        parent: Some(team_span),
        start,
        end: e.time,
        attrs,
        error: None,
    })
}

#[cfg(test)]
mod tests;
