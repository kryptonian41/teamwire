//! What the report tests share: the delivery rig (a store, a registry of
//! fakes, a clock) plus helpers for members, completions and events.
#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "helper functions in an integration-test file unwrap, and write the member's own report file as the member would; a failure there fails the test"
)]

// The including test file declares `rig` and `members` at its root, because
// the delivery support files name each other as `crate::rig`.
use serde_json::Value;
use teamwire_core::{Event, EventType, HarnessCaps, Role, Ulid};
use teamwire_fakes::FakeHarness;
use teamwire_store::{CompletionRecord, Party};

pub(crate) use crate::rig::{Rig, at};

/// A rig whose lead is in a healthy place, with member `api` (session `api-1`).
pub(crate) fn team() -> (Rig, Role) {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    let Party::Member(api) = rig.add_member("api", "fake", "api-1") else {
        unreachable!("add_member returns a member")
    };
    (rig, api)
}

impl Rig {
    pub(crate) fn newest(&self, who: &Role) -> CompletionRecord {
        self.store
            .read(|tx| tx.latest_completion(who))
            .unwrap()
            .unwrap()
    }

    pub(crate) fn completions(&self, who: &Role) -> usize {
        self.store
            .read(|tx| tx.completions_of(who))
            .unwrap()
            .items
            .len()
    }

    pub(crate) fn events(&self, kind: EventType) -> Vec<Event> {
        self.store.read(|tx| tx.events_of_type(kind)).unwrap().items
    }

    /// The harness without a turn-end hook.
    pub(crate) fn no_turn_end_hook(&self) {
        self.harness.set_caps(HarnessCaps {
            turn_end_hook: false,
            ..FakeHarness::caps()
        });
    }

    /// `$TEAMWIRE_HOME/teams/alpha`.
    pub(crate) fn team_dir(&self) -> std::path::PathBuf {
        self.home.path().join("teams").join("alpha")
    }

    pub(crate) fn write_report(&self, who: &Role, text: &str) {
        let dir = self.team_dir().join("reports");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{who}.md")), text).unwrap();
    }

    pub(crate) fn complete_now(&mut self, who: &Role) -> Ulid {
        let report = teamwire_report::done(
            &mut self.ctx(),
            who,
            teamwire_core::CompletionStatus::Done,
            "finished the api",
        )
        .unwrap();
        match report {
            teamwire_report::DoneReport::Stored { completion, .. } => completion,
            other @ teamwire_report::DoneReport::NotStored { .. } => {
                unreachable!("expected a stored completion, got {other:?}")
            }
        }
    }
}

pub(crate) fn data_str<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.data.get(key).and_then(Value::as_str)
}
