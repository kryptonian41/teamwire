//! A team's events, indexed so a metric or a span can find the other events
//! it depends on (a message's type, a generation's start) without a query.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use teamwire_core::{Event, EventType, TaskClass};

/// The task class of a member whose launch recorded none.
pub(crate) const UNCLASSIFIED: &str = "unclassified";

pub(crate) struct History {
    events: Vec<Event>,
    about: HashMap<String, Vec<usize>>,
    by_role: HashMap<String, Vec<usize>>,
    /// The `seq` of every reprompt a later undo took back.
    undone: HashSet<u64>,
}

impl History {
    /// `events` must be oldest first, as the store returns them.
    pub(crate) fn new(events: Vec<Event>) -> Self {
        let mut about: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_role: HashMap<String, Vec<usize>> = HashMap::new();
        let mut undone = HashSet::new();
        // Per role, the reprompts not yet undone; an undo pops the latest, as
        // `restore_generation` does, and a `member.creating` starts over.
        let mut open: HashMap<&str, Vec<u64>> = HashMap::new();
        for (i, e) in events.iter().enumerate() {
            if let Some(role) = &e.role {
                let stack = open.entry(role.as_str()).or_default();
                if e.kind == EventType::MemberCreating {
                    stack.clear();
                } else if matches!(e.kind, EventType::MemberReprompted | EventType::MemberWoken) {
                    // A wake starts a generation as a reprompt does (ADR 0043).
                    if flag(&e.data, "undone") {
                        undone.extend(stack.pop());
                    } else {
                        stack.push(e.seq);
                    }
                }
            }
            if let Some(subject) = &e.subject {
                about.entry(subject.to_string()).or_default().push(i);
            }
            if let Some(role) = &e.role {
                by_role.entry(role.as_str().to_owned()).or_default().push(i);
            }
        }
        Self {
            events,
            about,
            by_role,
            undone,
        }
    }

    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    /// Whether a later undo took back the reprompt `e`.
    pub(crate) fn is_undone(&self, e: &Event) -> bool {
        self.undone.contains(&e.seq)
    }

    /// The events whose subject is `subject`, oldest first.
    pub(crate) fn about(&self, subject: &str) -> impl Iterator<Item = &Event> {
        self.indexed(self.about.get(subject))
    }

    /// The events written for `role`, oldest first.
    pub(crate) fn of_role(&self, role: &str) -> impl Iterator<Item = &Event> {
        self.indexed(self.by_role.get(role))
    }

    fn indexed<'a>(&'a self, list: Option<&'a Vec<usize>>) -> impl Iterator<Item = &'a Event> {
        list.into_iter()
            .flat_map(|list| list.iter().filter_map(|&i| self.events.get(i)))
    }

    /// The `type` the message was sent with, from its `message.sent` event.
    pub(crate) fn message_type(&self, message: &str) -> Option<&str> {
        self.about(message)
            .find(|e| e.kind == EventType::MessageSent)
            .and_then(|e| text(&e.data, "type"))
    }

    /// The member kind recorded when `role` was created, as of `seq`.
    pub(crate) fn member_kind(&self, role: &str, seq: u64) -> &str {
        self.of_role(role)
            .take_while(|e| e.seq <= seq)
            .filter(|e| e.kind == EventType::MemberCreating)
            .filter_map(|e| text(&e.data, "kind"))
            .last()
            .unwrap_or("unknown")
    }

    /// The task class launch recorded for `role` on its latest
    /// `member.creating` as of `seq`, or `unclassified`. A team launched
    /// before classes existed recorded none. A value that is not one of the
    /// seven classes also reads as `unclassified`, so a label built from it
    /// keeps its seven values.
    pub(crate) fn task_class(&self, role: &str, seq: u64) -> &'static str {
        self.of_role(role)
            .take_while(|e| e.seq <= seq)
            .filter(|e| e.kind == EventType::MemberCreating)
            .last()
            .and_then(|e| text(&e.data, "task_class"))
            .and_then(|c| TaskClass::parse(c).ok())
            .map_or(UNCLASSIFIED, TaskClass::as_str)
    }

    /// The event that began the run `end` closes: the latest
    /// `member.started`, `member.handoff` or (not undone) `member.reprompted`
    /// or `member.woken` of the same role before it.
    pub(crate) fn generation_start(&self, end: &Event) -> Option<&Event> {
        let role = end.role.as_ref()?;
        self.of_role(role.as_str())
            .take_while(|e| e.seq < end.seq)
            .filter(|e| {
                matches!(e.kind, EventType::MemberStarted | EventType::MemberHandoff)
                    || (matches!(e.kind, EventType::MemberReprompted | EventType::MemberWoken)
                        && !flag(&e.data, "undone")
                        && !self.is_undone(e))
            })
            .last()
    }
}

pub(crate) fn text<'a>(data: &'a Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(Value::as_str)
}

pub(crate) fn number(data: &Value, key: &str) -> Option<f64> {
    data.get(key).and_then(Value::as_f64)
}

pub(crate) fn flag(data: &Value, key: &str) -> bool {
    data.get(key).and_then(Value::as_bool).unwrap_or(false)
}
