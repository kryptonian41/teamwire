//! What one message's events say about its latest delivery attempt to one
//! recipient, read once. Each caller asks a different question of the same
//! history, and the rules differ on purpose: staleness counts only the
//! latest resend, "in flight" also ends at a recorded outcome, a recipient's
//! stops count from the latest resend in commit order, and "unread" applies
//! the lead's view rules (ADR 0021, 0022). A sender that is still delivering
//! holds its rows `pending` too, so age alone is not enough: the clock
//! starts at the latest attempt, and a line typed twice is worse than a line
//! that waits another minute.

use std::time::Duration;

use teamwire_core::{Event, EventType, Message, Recipient, Role, Timestamp};
use teamwire_store::{ReadTx, StoreError};

use crate::line_holds_whole;

/// A `pending` delivery is stale only after this long without a new attempt.
pub const PENDING_STALE: Duration = Duration::from_secs(60);

/// The latest attempt at one message for one recipient, as its events
/// record it. The deliveries table keeps no attempt time of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptHistory {
    started: Timestamp,
    /// The start of an attempt with no outcome recorded after it.
    open_since: Option<Timestamp>,
    stops: usize,
    /// The typed line held the whole body (ADR 0021).
    whole: bool,
    /// The recipient whose completions can end "unread" (ADR 0022).
    member: Option<Role>,
}

impl AttemptHistory {
    /// Reads the events about `message` once, keeping those for `recipient`.
    /// Reads nothing else: only `unread_since` looks at completions.
    pub fn read(
        tx: &ReadTx<'_>,
        message: &Message,
        recipient: &Recipient,
    ) -> Result<Self, StoreError> {
        let events = tx.events_about(&message.ulid)?.all_or_corrupt()?;
        let mut history = Self::fold(message.created_at, recipient, &events);
        history.whole = line_holds_whole(&message.body);
        history.member = match recipient {
            Recipient::Member(role) => Some(role.clone()),
            Recipient::Lead | Recipient::Other(_) => None,
        };
        Ok(history)
    }

    /// Events are in commit order. `whole` and `member` are left for `read`.
    fn fold(created_at: Timestamp, recipient: &Recipient, events: &[Event]) -> Self {
        let mut history = Self {
            started: created_at,
            open_since: Some(created_at),
            stops: 0,
            whole: false,
            member: None,
        };
        for e in events.iter().filter(|e| is_for(e, recipient)) {
            if e.kind == EventType::DeliveryResent {
                // The latest by clock, not by commit order.
                history.started = history.started.max(e.time);
                history.open_since = Some(e.time.max(created_at));
                history.stops = 0;
            } else if matches!(
                e.kind,
                EventType::DeliveryHandedOff
                    | EventType::DeliveryUnconfirmed
                    | EventType::DeliveryUndelivered
                    | EventType::DeliveryAcked
            ) {
                history.open_since = None;
            } else if e.kind == EventType::DeliveryRecipientStopped {
                history.stops += 1;
            }
        }
        history
    }

    /// When the latest attempt began: the message's creation, or the latest
    /// `delivery.resent` for the recipient if later.
    pub fn started(&self) -> Timestamp {
        self.started
    }

    /// True when the latest attempt began over `PENDING_STALE` ago.
    pub fn is_stale(&self, now: Timestamp) -> bool {
        now.saturating_since(self.started) > PENDING_STALE
    }

    /// True when a command may still be delivering the message: the latest
    /// attempt began under `PENDING_STALE` ago and no outcome has been
    /// recorded since. Unlike `is_stale` this holds for a row in any state,
    /// since a retry leaves an `undelivered` or `unconfirmed` row in its
    /// state until its outcome is written.
    pub fn in_flight(&self, now: Timestamp) -> bool {
        self.open_since
            .is_some_and(|t| now.saturating_since(t) <= PENDING_STALE)
    }

    /// How many times the recipient's turn ended while the latest attempt
    /// was in flight (`delivery.recipient_stopped` since the message was
    /// sent or last resent, in commit order). Each such stop left the
    /// message to the sender, so a sender that would record `undelivered`
    /// tries again instead (ADR 0028).
    pub fn stops(&self) -> usize {
        self.stops
    }

    /// For a `handed_off` delivery, when the latest attempt began, or `None`
    /// when it is not unread for the lead's views: the typed line held the
    /// whole body (ADR 0021), or the member recorded a completion after the
    /// attempt began (ADR 0022). A member's own hooks keep their own rule
    /// (ADR 0021), because a hold before stopping is how a member is asked
    /// to read the full text at all. Reads the member's latest completion
    /// from `tx`, so callers that ask only the other questions never do.
    pub fn unread_since(&self, tx: &ReadTx<'_>) -> Result<Option<Timestamp>, StoreError> {
        let completed = match &self.member {
            Some(role) if !self.whole => {
                tx.latest_completion(role)?.map(|c| c.completion.created_at)
            }
            Some(_) | None => None,
        };
        Ok(unread_since(self.started, self.whole, completed))
    }
}

/// The attempt's start, unless the typed line held the whole body or the
/// member's latest completion came after the attempt began: the member
/// finished a turn that had the line, no later nudge exists, so an alert
/// would only send the lead to check by hand.
fn unread_since(
    started: Timestamp,
    whole: bool,
    completed: Option<Timestamp>,
) -> Option<Timestamp> {
    let finished_after = completed.is_some_and(|c| c > started);
    (!whole && !finished_after).then_some(started)
}

/// The event's `to` names the recipient. An event written before `to` was
/// recorded names a member by its role and the lead by no role; an agent in
/// another team also has no role, so only `to` can tell it from the lead.
fn is_for(event: &Event, recipient: &Recipient) -> bool {
    if let Some(to) = event.data.get("to").and_then(|v| v.as_str()) {
        return to == recipient.as_str();
    }
    match recipient {
        Recipient::Lead => event.role.is_none(),
        Recipient::Member(role) => event.role.as_ref() == Some(role),
        Recipient::Other(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use teamwire_core::{Role, Ulid};

    use super::*;

    use EventType::{
        DeliveryAcked as Acked, DeliveryHandedOff as HandedOff, DeliveryRecipientStopped as Stop,
        DeliveryResent as Resent, DeliveryUndelivered as Undelivered,
    };

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
    }

    fn event(seq: u64, secs: i64, kind: EventType, to: &str) -> Event {
        Event {
            seq,
            ulid: Ulid::from_parts(at(secs), u128::from(seq)),
            time: at(secs),
            kind,
            role: None,
            subject: None,
            data: json!({ "to": to }),
            writer_version: "test".to_owned(),
        }
    }

    struct Case {
        name: &'static str,
        events: &'static [(i64, EventType, &'static str)],
        now: i64,
        started: i64,
        stale: bool,
        in_flight: bool,
        stops: usize,
    }

    const STALE: i64 = 60;

    #[test]
    fn each_rule_reads_the_history_its_own_way() {
        // The message is created at 1 and sent to `inv`.
        let cases = [
            Case {
                name: "fresh send",
                events: &[],
                now: 1 + STALE,
                started: 1,
                stale: false,
                in_flight: true,
                stops: 0,
            },
            Case {
                name: "old send",
                events: &[],
                now: 2 + STALE,
                started: 1,
                stale: true,
                in_flight: false,
                stops: 0,
            },
            Case {
                name: "another recipient's events",
                events: &[(50, Resent, "a"), (51, Stop, "a"), (52, HandedOff, "a")],
                now: 30,
                started: 1,
                stale: false,
                in_flight: true,
                stops: 0,
            },
            Case {
                name: "an outcome ends the flight, not the clock",
                events: &[(5, Undelivered, "inv")],
                now: 30,
                started: 1,
                stale: false,
                in_flight: false,
                stops: 0,
            },
            Case {
                name: "a resend restarts the clock and the flight",
                events: &[(5, Undelivered, "inv"), (100, Resent, "inv")],
                now: 100 + STALE,
                started: 100,
                stale: false,
                in_flight: true,
                stops: 0,
            },
            Case {
                name: "an outcome after a resend",
                events: &[
                    (100, Resent, "inv"),
                    (101, HandedOff, "inv"),
                    (102, Acked, "inv"),
                ],
                now: 110,
                started: 100,
                stale: false,
                in_flight: false,
                stops: 0,
            },
            Case {
                name: "stops count",
                events: &[(2, Stop, "inv"), (3, Stop, "inv")],
                now: 4,
                started: 1,
                stale: false,
                in_flight: true,
                stops: 2,
            },
            Case {
                name: "a resend resets the stops",
                events: &[(2, Stop, "inv"), (3, Resent, "inv"), (4, Stop, "inv")],
                now: 5,
                started: 3,
                stale: false,
                in_flight: true,
                stops: 1,
            },
            Case {
                name: "an outcome keeps the stops",
                events: &[(2, Stop, "inv"), (3, Undelivered, "inv")],
                now: 5,
                started: 1,
                stale: false,
                in_flight: false,
                stops: 1,
            },
            // Commit order and clock order disagree: the clock's latest
            // resend starts the attempt, the last committed one opens it.
            Case {
                name: "an earlier-clocked resend committed last",
                events: &[(100, Resent, "inv"), (90, Resent, "inv")],
                now: 91 + STALE,
                started: 100,
                stale: false,
                in_flight: false,
                stops: 0,
            },
            Case {
                name: "a resend clocked before the message",
                events: &[(0, Resent, "inv")],
                now: 1 + STALE,
                started: 1,
                stale: false,
                in_flight: true,
                stops: 0,
            },
        ];
        let inv = Recipient::Member(Role::parse("inv").unwrap());
        for c in cases {
            let events: Vec<Event> = c
                .events
                .iter()
                .zip(1..)
                .map(|(&(secs, kind, to), seq)| event(seq, secs, kind, to))
                .collect();
            let h = AttemptHistory::fold(at(1), &inv, &events);
            let now = at(c.now);
            assert_eq!(h.started(), at(c.started), "{}: started", c.name);
            assert_eq!(h.is_stale(now), c.stale, "{}: stale", c.name);
            assert_eq!(h.in_flight(now), c.in_flight, "{}: in flight", c.name);
            assert_eq!(h.stops(), c.stops, "{}: stops", c.name);
        }
    }

    #[test]
    fn unread_is_the_attempt_start_unless_the_line_was_whole_or_the_member_finished_after() {
        // (whole line, latest completion, unread since); the attempt began at 10.
        let cases = [
            (false, None, Some(10)),
            (true, None, None),
            (false, Some(9), Some(10)),
            (false, Some(10), Some(10)),
            (false, Some(11), None),
            (true, Some(9), None),
        ];
        for (whole, completed, expected) in cases {
            assert_eq!(
                unread_since(at(10), whole, completed.map(at)),
                expected.map(at),
                "whole {whole}, completed {completed:?}"
            );
        }
    }

    #[test]
    fn an_event_without_to_is_for_the_lead_or_the_member_it_names() {
        let mut no_role = event(1, 5, Resent, "");
        no_role.data = json!({});
        let mut inv_role = no_role.clone();
        inv_role.role = Some(Role::parse("inv").unwrap());
        let inv = Recipient::Member(Role::parse("inv").unwrap());
        let other = Recipient::parse("db@beta").unwrap();
        assert!(is_for(&no_role, &Recipient::Lead));
        assert!(!is_for(&no_role, &inv));
        assert!(!is_for(&no_role, &other));
        assert!(is_for(&inv_role, &inv));
        assert!(!is_for(&inv_role, &Recipient::Lead));
    }
}
