//! What the report tests share: the delivery rig (a store, a registry of
//! fakes, a clock) plus helpers for members, completions and events.
#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "helper functions in an integration-test file unwrap, and write the member's own report file as the member would; a failure there fails the test"
)]

// The including test file declares `rig` and `members` at its root, because
// the delivery support files name each other as `crate::rig`.
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    Actor, DeliveryState, Event, EventType, HarnessCaps, HarnessId, Host, HostCaps, HostError,
    HostId, Key, MemberKind, MessageType, Mode, Occupant, OneLine, Place, PlaceState, Recipient,
    Registry, Role, Route, SpawnRequest, TurnEnd, Ulid,
};
use teamwire_fakes::{FakeHarness, FakeHost};
use teamwire_store::{CompletionRecord, DeliveryUpdate, NewMember, NewMessage, Party};

pub(crate) use crate::members::role;
pub(crate) use crate::rig::{Rig, at, sid};

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

/// The hook input of a plain stop by `session`.
pub(crate) fn stop(session: &str) -> TurnEnd {
    TurnEnd {
        session: sid(session),
        subagent: false,
        hold_already_active: false,
        final_message: Some("All done, the api is finished.".to_owned()),
        transcript: None,
    }
}

impl Rig {
    /// The lead's place (the only one before any member has one).
    pub(crate) fn lead_place(&self) -> Place {
        self.host
            .list(Duration::from_secs(1))
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    }

    /// Every line typed into the lead's place.
    pub(crate) fn typed_to_lead(&self) -> Vec<String> {
        self.host.submitted(&self.lead_place())
    }

    /// A message from the lead to `who`, handed off and not read.
    pub(crate) fn unread_from_lead(&mut self, who: &Role, body: &str) -> String {
        let at = at(2);
        let recipient = Recipient::Member(who.clone());
        self.store
            .write(|tx| {
                let stored = tx.insert_message(
                    at,
                    &NewMessage {
                        sender: Actor::Lead,
                        kind: MessageType::Note,
                        re: None,
                        commit: None,
                        body: body.to_owned(),
                        client_key: None,
                        recipients: vec![recipient.clone()],
                    },
                )?;
                tx.set_delivery_state(
                    at,
                    &stored.message.ulid,
                    &recipient,
                    &DeliveryUpdate {
                        state: DeliveryState::HandedOff,
                        route: Some(Route::HostTyping),
                        error: None,
                        attempts: 1,
                    },
                )?;
                Ok(stored.message.display_id.to_string())
            })
            .unwrap()
    }

    /// Replaces the registry's host with one that runs `before(op, timeout)`
    /// ahead of every call.
    /// An implementer in a worktree (`cwd`) started from `base_sha`, so the
    /// report check reads its git history.
    pub(crate) fn add_worktree_member(
        &mut self,
        name: &str,
        session: &str,
        base_sha: &str,
    ) -> Role {
        let r = role(name);
        let session = sid(session);
        let cwd = self.home.path().to_path_buf();
        self.store
            .write(|tx| {
                tx.insert_member(
                    at(1),
                    &NewMember {
                        role: r.clone(),
                        title: name.to_owned(),
                        kind: MemberKind::Implementer,
                        mode: Mode::Worktree,
                        harness: HarnessId::from_static("fake"),
                        host: HostId::from_static("fake"),
                        cwd,
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: Some(base_sha.to_owned()),
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_session(&r, &session)
            })
            .unwrap();
        r
    }

    pub(crate) fn wrap_host(
        &mut self,
        before: impl Fn(&'static str, Duration) + Send + Sync + 'static,
    ) {
        let wrapped = Arc::new(WrapHost {
            inner: self.host.clone(),
            before: Box::new(before),
        });
        let mut registry = Registry::new();
        registry.add_harness(self.harness.clone()).unwrap();
        registry.add_host(wrapped).unwrap();
        self.registry = registry;
    }
}

type Before = Box<dyn Fn(&'static str, Duration) + Send + Sync>;

/// A host that runs a closure before each call, then does what the fake does.
struct WrapHost {
    inner: Arc<FakeHost>,
    before: Before,
}

impl Host for WrapHost {
    fn id(&self) -> HostId {
        self.inner.id()
    }
    fn capabilities(&self) -> HostCaps {
        self.inner.capabilities()
    }
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
        (self.before)("spawn", timeout);
        self.inner.spawn(req, timeout)
    }
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError> {
        (self.before)("submit", timeout);
        self.inner.submit(place, text, timeout)
    }
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        (self.before)("state", timeout);
        self.inner.state(place, timeout)
    }
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
        (self.before)("occupant", timeout);
        self.inner.occupant(place, timeout)
    }
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        (self.before)("read_tail", timeout);
        self.inner.read_tail(place, lines, timeout)
    }
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        (self.before)("send_keys", timeout);
        self.inner.send_keys(place, keys, timeout)
    }
    fn close(&self, place: &Place, timeout: Duration) -> Result<(), HostError> {
        (self.before)("close", timeout);
        self.inner.close(place, timeout)
    }
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        (self.before)("list", timeout);
        self.inner.list(timeout)
    }
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        (self.before)("live_agent_names", timeout);
        self.inner.live_agent_names(timeout)
    }
    fn place_env_var(&self) -> Option<&'static str> {
        self.inner.place_env_var()
    }
}
