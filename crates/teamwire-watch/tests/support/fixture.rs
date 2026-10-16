//! A team of fake members and a watch to poll them: the store, the in-process
//! fakes, a clock that only moves when a test says, and the watch's memory.
#![expect(
    clippy::unwrap_used,
    reason = "test support: a failure here fails the test"
)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Actor, Clock, CompletionStatus, Config, DeliveryError, DeliveryErrorCode, FinalSource,
    HarnessId, HostId, LeadRoute, Liveness, Member, MemberKind, MemberState, MessageType, Mode,
    Occupant, Place, PlaceState, Recipient, Registry, ReportState, Role, Route, SessionId,
    TeamName, Ulid,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_store::{Ctx, NewCompletion, NewMember, NewMessage, NewTeam, ResultOutcome, Store};
use teamwire_watch::WatchMemory;

pub const TEAM: &str = "alpha";
/// One poll's worth of time.
pub const POLL: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub struct Bed {
    pub home: TestHome,
    pub store: Store,
    pub clock: FakeClock,
    pub platform: FakePlatform,
    pub registry: Registry,
    pub config: Config,
    pub harness: Arc<FakeHarness>,
    pub host: Arc<FakeHost>,
    pub memory: WatchMemory,
    places: HashMap<String, Place>,
}

pub fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

impl Bed {
    pub fn new() -> Self {
        Self::with_route(LeadRoute::Sink)
    }

    pub fn with_route(route: LeadRoute) -> Self {
        let home = TestHome::new().unwrap();
        let team = TeamName::parse(TEAM).unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        let clock = FakeClock::new();
        let now = clock.now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: team.clone(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static(FakeHarness::ID),
                        lead_host: HostId::from_static(FakeHost::ID),
                        lead_route: route,
                        lead_session: None,
                    },
                )
                .map(|_| ())
            })
            .unwrap();
        let harness = Arc::new(FakeHarness::new());
        let host = Arc::new(FakeHost::new());
        let mut registry = Registry::new();
        registry.add_harness(harness.clone()).unwrap();
        registry.add_host(host.clone()).unwrap();
        Self {
            home,
            store,
            clock,
            platform: FakePlatform::new(),
            registry,
            config: Config::default(),
            harness,
            host,
            memory: WatchMemory::default(),
            places: HashMap::new(),
        }
    }

    fn place(&self, name: &str) -> &Place {
        self.places
            .get(name)
            .unwrap_or_else(|| unreachable!("no member {name} was added"))
    }

    pub fn session(name: &str) -> SessionId {
        SessionId::parse(&format!("sess-{name}")).unwrap()
    }

    /// Adds a member that has started, is running and looks idle. Its
    /// deadline is 1800 s.
    pub fn add(&mut self, name: &str) -> Member {
        self.add_with_deadline(name, 1800)
    }

    pub fn add_with_deadline(&mut self, name: &str, deadline_s: u64) -> Member {
        self.insert(name, deadline_s, true, None)
    }

    /// Adds a started member whose `member.started` event was written by
    /// teamwire `version`.
    pub fn add_started_by(&mut self, name: &str, version: &str) -> Member {
        self.insert(name, 1800, true, Some(version))
    }

    /// Adds a member whose launch stopped in `creating`: no place, no
    /// session, never started.
    pub fn add_creating(&mut self, name: &str) {
        self.insert(name, 1800, false, None);
    }

    fn insert(&mut self, name: &str, deadline_s: u64, start: bool, by: Option<&str>) -> Member {
        let role = role(name);
        let session = Self::session(name);
        let place = self.host.add_place(
            name,
            PlaceState::Idle,
            Occupant {
                session: None,
                pids: vec![],
            },
        );
        self.harness
            .add_session(session.clone(), vec![], Liveness::Alive(PlaceState::Idle));
        let now = self.clock.now();
        let new = NewMember {
            role: role.clone(),
            title: format!("the {name}"),
            kind: MemberKind::Implementer,
            mode: Mode::Pane,
            harness: HarnessId::from_static(FakeHarness::ID),
            host: HostId::from_static(FakeHost::ID),
            cwd: self.home.path().to_path_buf(),
            repo: None,
            branch: Some("work".to_owned()),
            base: None,
            base_sha: None,
            check: None,
            owns: vec![],
            deadline_s,
        };
        if !start {
            self.store.write(|tx| tx.insert_member(now, &new)).unwrap();
            return self.member(name);
        }
        let team = TeamName::parse(TEAM).unwrap();
        let mut other = by.map(|v| {
            Store::open(self.home.path(), &team)
                .unwrap()
                .with_writer_version(v)
        });
        let store = other.as_mut().unwrap_or(&mut self.store);
        store
            .write(|tx| {
                tx.insert_member(now, &new)?;
                tx.set_member_place(&role, &place)?;
                tx.set_member_session(&role, &session)?;
                tx.set_member_state(now, &role, MemberState::Started, &json!({}))
            })
            .unwrap();
        self.places.insert(name.to_owned(), place);
        self.member(name)
    }

    pub fn member(&self, name: &str) -> Member {
        let role = role(name);
        self.store.read(|tx| tx.member(&role)).unwrap().unwrap()
    }

    /// What the member's harness and host both report from now on.
    pub fn show(&self, name: &str, state: PlaceState) {
        self.harness
            .set_liveness(&Self::session(name), Liveness::Alive(state));
        self.host.set_state(self.place(name), state);
    }

    pub fn working(&self, name: &str) {
        self.show(name, PlaceState::Working);
    }

    pub fn idle(&self, name: &str) {
        self.show(name, PlaceState::Idle);
    }

    pub fn blocked(&self, name: &str) {
        self.show(name, PlaceState::Blocked);
    }

    /// The harness no longer knows the member's session. The host's place
    /// stays, so the host's listing is not empty.
    pub fn vanish(&self, name: &str) {
        self.harness
            .set_liveness(&Self::session(name), Liveness::Dead);
    }

    pub fn age(&self, by: Duration) {
        self.clock.advance(by);
    }

    /// One poll now, at the current time.
    pub fn poll(&mut self) -> Option<String> {
        let mut ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        teamwire_watch::poll(&mut ctx, &mut self.memory).map(teamwire_core::OneLine::into_string)
    }

    /// Starts a watch as live process `pid`, which the fake platform reports alive.
    pub fn start_watch(&mut self, pid: u32) {
        let pid = teamwire_core::Pid::new(pid);
        self.platform.set_alive(pid, true);
        let mut ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        teamwire_watch::start(&mut ctx, &mut WatchMemory::default(), pid, "testhost").unwrap();
    }

    /// One poll interval later, one poll.
    pub fn tick(&mut self) -> Option<String> {
        self.clock.advance(POLL);
        self.poll()
    }

    /// `n` ticks; the lines that were printed.
    pub fn ticks(&mut self, n: usize) -> Vec<String> {
        (0..n).filter_map(|_| self.tick()).collect()
    }

    /// Stores a completion for the member's current generation.
    pub fn complete(&mut self, name: &str, status: CompletionStatus) -> Ulid {
        let member = self.member(name);
        let now = self.clock.now();
        let new = NewCompletion {
            role: member.role.clone(),
            generation: member.generation,
            status,
            summary: format!("{name} finished"),
            final_message: None,
            final_source: FinalSource::Hook,
        };
        self.store
            .write(|tx| tx.insert_completion(now, &new).map(|c| c.ulid))
            .unwrap()
    }

    /// Ends the delivery of a completion as the hook would.
    pub fn finish(
        &mut self,
        completion: &Ulid,
        state: ReportState,
        code: Option<DeliveryErrorCode>,
    ) {
        let now = self.clock.now();
        let outcome = ResultOutcome {
            state,
            route: (state == ReportState::Delivered).then_some(Route::HostTyping),
            error: code.map(|c| DeliveryError::new(c, "test")),
            result_sha256: None,
        };
        self.store
            .write(|tx| {
                tx.claim_completion_for_delivery(now, completion, Duration::from_secs(90))?;
                tx.finish_completion_delivery(now, completion, &outcome)
                    .map(|_| ())
            })
            .unwrap();
    }

    /// A member that finished and whose result reached the lead.
    pub fn delivered(&mut self, name: &str, status: CompletionStatus) -> Ulid {
        let id = self.complete(name, status);
        self.finish(&id, ReportState::Delivered, None);
        id
    }

    /// A question from `name` to the lead, delivered as `state`.
    pub fn question(&mut self, name: &str, state: teamwire_core::DeliveryState) -> String {
        self.question_from(Actor::Member(role(name)), state)
    }

    /// A question to the lead from `sender`, which may be an agent of
    /// another team, left in `state`.
    pub fn question_from(&mut self, sender: Actor, state: teamwire_core::DeliveryState) -> String {
        let now = self.clock.now();
        let new = NewMessage {
            sender,
            kind: MessageType::Question,
            re: None,
            commit: None,
            body: "which branch?".to_owned(),
            client_key: None,
            recipients: vec![Recipient::Lead],
        };
        self.store
            .write(|tx| {
                let stored = tx.insert_message(now, &new)?;
                tx.set_delivery_state(
                    now,
                    &stored.message.ulid,
                    &Recipient::Lead,
                    &teamwire_store::DeliveryUpdate {
                        state,
                        route: None,
                        error: None,
                        attempts: 1,
                    },
                )?;
                Ok(stored.message.display_id.to_string())
            })
            .unwrap()
    }

    pub fn reprompt(&mut self, name: &str) {
        let now = self.clock.now();
        let role = role(name);
        self.store
            .write(|tx| tx.bump_generation(now, &role, "more work"))
            .unwrap();
    }

    pub fn remove(&mut self, name: &str) {
        let now = self.clock.now();
        let role = role(name);
        self.store
            .write(|tx| {
                tx.remove_member(now, &role, Some("removed"), &json!({}))
                    .map(|_| ())
            })
            .unwrap();
    }
}

impl Default for Bed {
    fn default() -> Self {
        Self::new()
    }
}

/// The event names in a line, in order: `LATE`, `TEAM-STALLED`, ...
pub fn names(line: &str) -> Vec<String> {
    line.split(" | ")
        .map(|part| {
            part.strip_prefix(&format!("[teamwire:{TEAM}] "))
                .unwrap_or(part)
                .split(' ')
                .next()
                .unwrap_or("")
                .to_owned()
        })
        .collect()
}

/// How many times each event was printed across `lines`.
pub fn counts(lines: &[String]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for line in lines {
        for name in names(line) {
            *out.entry(name).or_insert(0) += 1;
        }
    }
    out
}

pub fn count(lines: &[String], name: &str) -> usize {
    counts(lines).get(name).copied().unwrap_or(0)
}
