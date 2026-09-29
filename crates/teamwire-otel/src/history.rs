//! A team's events, indexed so a metric or a span can find the other events
//! it depends on (a message's type, a generation's start) without a query.

use std::collections::HashMap;

use serde_json::Value;
use teamwire_core::{Event, EventType};

pub(crate) struct History {
    events: Vec<Event>,
    by_ulid: HashMap<String, usize>,
    about: HashMap<String, Vec<usize>>,
    by_role: HashMap<String, Vec<usize>>,
}

impl History {
    /// `events` must be oldest first, as the store returns them.
    pub(crate) fn new(events: Vec<Event>) -> Self {
        let mut by_ulid = HashMap::new();
        let mut about: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_role: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in events.iter().enumerate() {
            by_ulid.insert(e.ulid.to_string(), i);
            if let Some(subject) = &e.subject {
                about.entry(subject.to_string()).or_default().push(i);
            }
            if let Some(role) = &e.role {
                by_role.entry(role.as_str().to_owned()).or_default().push(i);
            }
        }
        Self {
            events,
            by_ulid,
            about,
            by_role,
        }
    }

    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    /// One event by its ULID: how a span's start is found from its end.
    pub(crate) fn get(&self, ulid: &str) -> Option<&Event> {
        self.by_ulid.get(ulid).map(|&i| &self.events[i])
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
            .flat_map(|list| list.iter().map(|&i| &self.events[i]))
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

    /// The event that began the run `end` closes: the latest
    /// `member.started` or (not undone) `member.reprompted` of the same role
    /// before it.
    pub(crate) fn generation_start(&self, end: &Event) -> Option<&Event> {
        let role = end.role.as_ref()?;
        self.of_role(role.as_str())
            .take_while(|e| e.seq < end.seq)
            .filter(|e| match e.kind {
                EventType::MemberStarted => true,
                EventType::MemberReprompted => !flag(&e.data, "undone"),
                _ => false,
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
