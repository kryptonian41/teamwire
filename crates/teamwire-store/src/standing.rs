//! Which event started a member's generation, once undone bumps are taken
//! out (ADR 0059). A reprompt whose delivery failed is bumped and then
//! undone (`restore_generation`), so the newest `member.reprompted` event
//! can be an undo while an older reprompt still owns the generation. Never
//! read the newest event alone: replay the history.

use serde_json::Value;
use teamwire_core::{Event, EventType, Role};

use crate::error::StoreError;
use crate::tx::ReadTx;

/// The events that start a generation.
const STARTS: [EventType; 3] = [
    EventType::MemberReprompted,
    EventType::MemberWoken,
    EventType::MemberHandoff,
];

impl ReadTx<'_> {
    /// The event that started `role`'s generation `generation` and still
    /// stands: a reprompt, a wake or a handoff that no later undo took back.
    /// `None` for a generation that launch started, or one whose start was
    /// undone. Fails only when one of those events is damaged.
    pub fn generation_start(
        &self,
        role: &Role,
        generation: u32,
    ) -> Result<Option<Event>, StoreError> {
        Ok(standing(self.events_of_kinds(role, &STARTS)?)
            .into_iter()
            .rfind(|e| generation_of(e) == Some(generation)))
    }

    /// The generation message `display_id` started for `role` and that still
    /// stands. `None` when the message started none for `role`, or the bump
    /// was undone after its delivery failed.
    pub fn generation_started_by(
        &self,
        role: &Role,
        display_id: &str,
    ) -> Result<Option<u32>, StoreError> {
        Ok(standing(self.events_of_kinds(role, &STARTS)?)
            .iter()
            .rfind(|e| e.data.get("message").and_then(Value::as_str) == Some(display_id))
            .and_then(generation_of))
    }
}

fn generation_of(e: &Event) -> Option<u32> {
    e.data
        .get("generation")
        .and_then(Value::as_u64)
        .and_then(|g| u32::try_from(g).ok())
}

/// The start events that still stand, oldest first. An undo (`"undone":
/// true`, with the generation it restored) takes back the newest standing
/// bump of its kind one generation above it.
fn standing(events: Vec<Event>) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    for e in events {
        if e.data.get("undone").and_then(Value::as_bool) == Some(true) {
            let restored = generation_of(&e);
            if let Some(at) = out.iter().rposition(|b| {
                b.kind == e.kind && generation_of(b) == restored.and_then(|g| g.checked_add(1))
            }) {
                out.remove(at);
            }
            continue;
        }
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use teamwire_core::EventType;

    use crate::testkit::{Fixture, at, role, with_members};

    /// m001 starts generation 2; m002 bumps to 3 and is undone back to 2.
    fn rolled_back() -> Fixture {
        let mut f = with_members(&["api"]);
        let r = role("api");
        f.store
            .write(|tx| {
                tx.bump_generation(at(10), &r, "m001")?;
                tx.bump_generation(at(20), &r, "m002")?;
                tx.restore_generation(at(21), &r, 2, at(10), EventType::MemberReprompted)
            })
            .unwrap();
        f
    }

    #[test]
    fn an_undone_bump_leaves_the_older_reprompt_owning_the_generation() {
        let f = rolled_back();
        let r = role("api");
        let start = f
            .store
            .read(|tx| tx.generation_start(&r, 2))
            .unwrap()
            .unwrap();
        assert_eq!(
            start.data.get("message").and_then(|m| m.as_str()),
            Some("m001")
        );
        assert_eq!(f.store.read(|tx| tx.generation_start(&r, 3)).unwrap(), None);
    }

    #[test]
    fn a_reprompt_names_the_generation_it_started_unless_it_was_undone() {
        let f = rolled_back();
        let r = role("api");
        let started = |id: &str| f.store.read(|tx| tx.generation_started_by(&r, id)).unwrap();
        assert_eq!(started("m001"), Some(2));
        assert_eq!(started("m002"), None);
        assert_eq!(started("m003"), None);
    }
}
