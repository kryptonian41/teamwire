//! One event in, the metric observations it counts as out (`docs/metrics.md`
//! sections 2 and 5). Local metrics and the export both read this, so the two
//! can never disagree about what an event means.

use crate::spans::recipient_of;
use serde_json::Value;
use teamwire_core::{Event, EventType, Timestamp};

use crate::defs::{self, Def};
use crate::history::{History, flag, number, text};

/// One counted event, or one measured duration, at the event's own time.
#[derive(Debug, Clone)]
pub(crate) struct Observation {
    pub(crate) def: &'static Def,
    pub(crate) labels: Vec<(String, String)>,
    pub(crate) value: f64,
    pub(crate) time: Timestamp,
}

const LAUNCH_STEPS: [&str; 5] = ["worktree", "setup", "spawn", "trust", "dispatch"];

/// Delivery codes the locator (ADR 0006) refuses with.
const LOCATOR_CODES: [&str; 10] = [
    "no_live_process",
    "no_place",
    "host_other_session",
    "host_no_session",
    "host_listing_failed",
    "two_places",
    "single_source",
    "background_session",
    "session_blocked",
    "not_at_input_prompt",
];

fn labels(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

struct Out<'a> {
    event: &'a Event,
    team: &'a str,
    list: Vec<Observation>,
}

impl Out<'_> {
    /// A count of one. `extra` follows the `team` label.
    fn count(&mut self, def: &'static Def, extra: &[(&str, &str)]) {
        self.push(def, 1.0, extra, true);
    }

    fn measure(&mut self, def: &'static Def, value: f64, extra: &[(&str, &str)]) {
        self.push(def, value, extra, true);
    }

    fn push(&mut self, def: &'static Def, value: f64, extra: &[(&str, &str)], team: bool) {
        let mut all = if team {
            labels(&[("team", self.team)])
        } else {
            Vec::new()
        };
        all.extend(labels(extra));
        self.list.push(Observation {
            def,
            labels: all,
            value,
            time: self.event.time,
        });
    }
}

/// What `event` counts as. `team` is the team's name.
pub(crate) fn observe(h: &History, event: &Event, team: &str) -> Vec<Observation> {
    let mut o = Out {
        event,
        team,
        list: Vec::new(),
    };
    let d = &event.data;
    match event.kind {
        EventType::MemberStarted | EventType::MemberFailed => launch(&mut o, d),
        EventType::MemberReprompted if !flag(d, "undone") => {
            let kind = kind_of(h, event);
            o.count(&defs::MEMBER_REPROMPTS, &[("kind", kind)]);
        }
        EventType::MemberCompleted => completed(h, &mut o),
        EventType::MemberCompletionRefused => {
            let reason = text(d, "reason").unwrap_or("unknown");
            o.count(&defs::COMPLETIONS_REFUSED, &[("reason", reason)]);
        }
        EventType::MemberHeld => held(&mut o, d),
        EventType::MessageSent => {
            let kind = text(d, "type").unwrap_or("unknown");
            let sender = text(d, "sender_kind").unwrap_or("unknown");
            o.count(
                &defs::MESSAGES_SENT,
                &[("type", kind), ("sender_kind", sender)],
            );
        }
        EventType::MessageRefused => {
            let kind = text(d, "type").unwrap_or("unknown");
            let reason = text(d, "reason")
                .or_else(|| text(d, "code"))
                .unwrap_or("unknown");
            o.count(
                &defs::MESSAGES_REFUSED,
                &[("type", kind), ("reason", reason)],
            );
        }
        EventType::DeliveryHandedOff => delivery(h, &mut o, "handed_off"),
        EventType::DeliveryUnconfirmed => delivery(h, &mut o, "unconfirmed"),
        EventType::DeliveryUndelivered => delivery(h, &mut o, "undelivered"),
        EventType::DeliveryAcked => acked(h, &mut o),
        EventType::ResultDelivered => result(&mut o, "delivered"),
        EventType::ResultUnconfirmed => result(&mut o, "unconfirmed"),
        EventType::ResultUndelivered => result(&mut o, "undelivered"),
        EventType::WatchLine => watch_line(&mut o, d),
        EventType::VerifyRecorded => verify(&mut o, d),
        EventType::TeamClosed => {
            let outcome = text(d, "outcome").unwrap_or("unknown");
            let step = text(d, "failed_step").unwrap_or("none");
            o.push(
                &defs::CLOSE_RUNS,
                1.0,
                &[("outcome", outcome), ("failed_step", step)],
                false,
            );
        }
        EventType::StoreError => {
            let op = text(d, "op").unwrap_or("unknown");
            let code = text(d, "code").unwrap_or("unknown");
            o.push(
                &defs::STORE_ERRORS,
                1.0,
                &[("op", op), ("code", code)],
                false,
            );
        }
        EventType::TeamCreated
        | EventType::TeamClosing
        | EventType::TeamLeadAdopted
        | EventType::MemberCreating
        | EventType::MemberReplaced
        | EventType::MemberRemoved
        | EventType::MemberReprompted
        | EventType::DeliveryReminded
        | EventType::DeliveryResent
        | EventType::DeliveryReentered
        | EventType::ResultDelivering
        | EventType::WatchStarted
        | EventType::WatchStopped
        | EventType::VerifyWaived
        | EventType::CloseStep
        | EventType::CloseRefused => {}
    }
    if let Some(ms) = number(d, "hook_ms") {
        let hook = text(d, "hook").unwrap_or("unknown");
        o.push(&defs::HOOK_DURATION, ms / 1000.0, &[("hook", hook)], false);
    }
    o.list
}

fn kind_of<'a>(h: &'a History, e: &Event) -> &'a str {
    e.role
        .as_ref()
        .map_or("unknown", |role| h.member_kind(role.as_str(), e.seq))
}

/// The `reason_code` of a failed launch when it is a short code. The free
/// `reason` text can hold a path, so it is never a label.
fn reason_code(d: &Value) -> &str {
    text(d, "reason_code")
        .filter(|c| {
            (1..=32).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        })
        .unwrap_or("other")
}

fn launch(o: &mut Out<'_>, d: &Value) {
    if o.event.kind == EventType::MemberStarted {
        o.count(
            &defs::LAUNCH_MEMBERS,
            &[("outcome", "started"), ("reason", "none")],
        );
    } else {
        o.count(
            &defs::LAUNCH_MEMBERS,
            &[("outcome", "failed"), ("reason", reason_code(d))],
        );
    }
    for step in LAUNCH_STEPS {
        if let Some(ms) = d.get("steps").and_then(|s| number(s, step)) {
            o.measure(&defs::LAUNCH_DURATION, ms / 1000.0, &[("step", step)]);
        }
    }
}

fn completed(h: &History, o: &mut Out<'_>) {
    let d = &o.event.data;
    let kind = kind_of(h, o.event);
    let status = text(d, "status").unwrap_or("unknown");
    o.count(&defs::COMPLETIONS, &[("kind", kind), ("status", status)]);
    if let Some(start) = h.generation_start(o.event) {
        let seconds = o.event.time.saturating_since(start.time).as_secs_f64();
        o.measure(
            &defs::MEMBER_RUN_DURATION,
            seconds,
            &[("kind", kind), ("status", status)],
        );
    }
}

fn held(o: &mut Out<'_>, d: &Value) {
    // `gaps` is the core `ReportGaps` object.
    if let Some(gaps) = d.get("gaps") {
        let listed = |key: &str| {
            gaps.get(key)
                .and_then(Value::as_array)
                .is_some_and(|list| !list.is_empty())
        };
        for (present, reason) in [
            (flag(gaps, "report_missing"), "report_missing"),
            (listed("commits_without_line"), "progress_lines"),
            (flag(gaps, "decisions_missing"), "decisions"),
        ] {
            if present {
                o.count(&defs::HOLDS, &[("reason", reason)]);
            }
        }
    }
    let unread = match d.get("unread") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) > 0.0,
        Some(Value::Array(a)) => !a.is_empty(),
        _ => false,
    };
    if unread {
        o.count(&defs::HOLDS, &[("reason", "unread")]);
    }
}

fn delivery(h: &History, o: &mut Out<'_>, outcome: &str) {
    let d = &o.event.data;
    let route = text(d, "route").unwrap_or("none");
    o.count(&defs::DELIVERIES, &[("route", route), ("outcome", outcome)]);
    if let Some(seconds) = number(d, "seconds") {
        o.measure(
            &defs::DELIVERY_DURATION,
            seconds,
            &[("route", route), ("outcome", outcome)],
        );
    }
    if let Some(attempts) = number(d, "attempts") {
        o.measure(&defs::DELIVERY_ATTEMPTS, attempts, &[("route", route)]);
    }
    if outcome == "undelivered" {
        let code = text(d, "code").unwrap_or("unknown");
        o.count(&defs::DELIVERIES_UNDELIVERED, &[("code", code)]);
        if LOCATOR_CODES.contains(&code) {
            o.count(&defs::LOCATOR_REFUSALS, &[("code", code)]);
        }
    }
    // A question handed to the lead costs the lead one full turn.
    let to_lead = o.event.role.is_none();
    let is_question = o
        .event
        .subject
        .as_ref()
        .and_then(|m| h.message_type(&m.to_string()))
        == Some("question");
    if outcome == "handed_off" && to_lead && is_question {
        o.count(&defs::LEAD_WAKEUPS, &[("source", "question")]);
    }
}

fn acked(h: &History, o: &mut Out<'_>) {
    let kind = o
        .event
        .subject
        .as_ref()
        .and_then(|m| h.message_type(&m.to_string()))
        .unwrap_or("unknown");
    o.count(&defs::MESSAGES_ACKED, &[("type", kind)]);
    if let Some(seconds) = number(&o.event.data, "seconds") {
        o.measure(&defs::ACK_DURATION, seconds, &[("type", kind)]);
    }
}

fn result(o: &mut Out<'_>, outcome: &str) {
    let d = &o.event.data;
    let route = text(d, "route").unwrap_or("none");
    o.count(&defs::RESULTS, &[("route", route), ("outcome", outcome)]);
    if let Some(seconds) = number(d, "seconds") {
        o.measure(&defs::RESULT_DURATION, seconds, &[("route", route)]);
    }
    // A result that came as a watch line is counted with the line itself.
    if outcome == "delivered" && route != "watch" {
        o.count(&defs::LEAD_WAKEUPS, &[("source", "result")]);
    }
}

fn watch_line(o: &mut Out<'_>, d: &Value) {
    o.count(&defs::WATCH_LINES, &[]);
    o.count(&defs::LEAD_WAKEUPS, &[("source", "watch")]);
    if let Some(events) = d.get("events").and_then(Value::as_array) {
        for name in events.iter().filter_map(Value::as_str) {
            o.count(&defs::WATCH_EVENTS, &[("event", name)]);
        }
    }
}

fn verify(o: &mut Out<'_>, d: &Value) {
    let verdict = text(d, "verdict").unwrap_or("unknown");
    o.count(&defs::VERIFY_RUNS, &[("verdict", verdict)]);
    if let Some(flags) = d.get("flags").and_then(Value::as_array) {
        for id in flags.iter().filter_map(Value::as_str) {
            o.count(&defs::VERIFY_FLAGS, &[("flag", id)]);
        }
    }
    if let Some(seconds) = number(d, "seconds") {
        o.measure(
            &defs::VERIFY_CHECK_DURATION,
            seconds,
            &[("verdict", verdict)],
        );
    }
}

/// Gauges as of the events up to `upto_seq`, stamped `at`.
pub(crate) fn gauges(h: &History, upto_seq: u64, at: Timestamp, team: &str) -> Vec<Observation> {
    let mut list = Vec::new();
    let mut gauge = |def: &'static Def, extra: &[(&str, &str)], value: usize| {
        let mut all = labels(&[("team", team)]);
        all.extend(labels(extra));
        list.push(Observation {
            def,
            labels: all,
            #[expect(
                clippy::cast_precision_loss,
                reason = "a count of members or messages is far below 2^52"
            )]
            value: value as f64,
            time: at,
        });
    };

    let mut state: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for e in h.events().iter().take_while(|e| e.seq <= upto_seq) {
        // A completed close ends every member, and writes no per-member event.
        if e.kind == EventType::TeamClosed && text(&e.data, "outcome") == Some("completed") {
            state.clear();
            continue;
        }
        let (Some(role), Some(s)) = (e.role.as_ref(), member_state(e.kind)) else {
            continue;
        };
        state.insert(role.as_str(), s);
    }
    for s in ["creating", "started"] {
        gauge(
            &defs::MEMBERS_LIVE,
            &[("state", s)],
            state.values().filter(|v| **v == s).count(),
        );
    }

    let (handed, acked) = read_state(h, upto_seq);
    for kind in ["lead", "member"] {
        let unread = handed
            .iter()
            .filter(|(k, _)| *k == kind)
            .filter(|(_, key)| !acked.contains(key))
            .count();
        gauge(&defs::MESSAGES_UNREAD, &[("recipient_kind", kind)], unread);
    }
    list
}

/// The state a member event moves its role to.
fn member_state(kind: EventType) -> Option<&'static str> {
    match kind {
        EventType::MemberCreating => Some("creating"),
        EventType::MemberStarted => Some("started"),
        EventType::MemberFailed => Some("failed"),
        EventType::MemberRemoved => Some("removed"),
        EventType::TeamCreated
        | EventType::TeamClosing
        | EventType::TeamClosed
        | EventType::TeamLeadAdopted
        | EventType::MemberReplaced
        | EventType::MemberReprompted
        | EventType::MemberCompleted
        | EventType::MemberCompletionRefused
        | EventType::MemberHeld
        | EventType::MessageSent
        | EventType::MessageRefused
        | EventType::DeliveryHandedOff
        | EventType::DeliveryUnconfirmed
        | EventType::DeliveryUndelivered
        | EventType::DeliveryAcked
        | EventType::DeliveryReminded
        | EventType::DeliveryResent
        | EventType::DeliveryReentered
        | EventType::ResultDelivering
        | EventType::ResultDelivered
        | EventType::ResultUndelivered
        | EventType::ResultUnconfirmed
        | EventType::WatchStarted
        | EventType::WatchLine
        | EventType::WatchStopped
        | EventType::VerifyRecorded
        | EventType::VerifyWaived
        | EventType::CloseStep
        | EventType::CloseRefused
        | EventType::StoreError => None,
    }
}

/// The deliveries handed off, as (recipient kind, message and recipient), and
/// the keys among them that were read.
pub(crate) fn read_state(
    h: &History,
    upto_seq: u64,
) -> (
    Vec<(&'static str, String)>,
    std::collections::HashSet<String>,
) {
    let key = |e: &Event| {
        format!(
            "{}/{}",
            e.subject
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            recipient_of(e)
        )
    };
    let mut handed = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut acked = std::collections::HashSet::new();
    for e in h.events().iter().take_while(|e| e.seq <= upto_seq) {
        let to = recipient_of(e);
        // A row for an agent of another team is read in that team's store
        // and counted there; this team's row never becomes `acked`.
        if to.contains('@') {
            continue;
        }
        if e.kind == EventType::DeliveryHandedOff && seen.insert(key(e)) {
            let kind = if to == "lead" { "lead" } else { "member" };
            handed.push((kind, key(e)));
        } else if e.kind == EventType::DeliveryAcked {
            acked.insert(key(e));
        }
    }
    (handed, acked)
}
