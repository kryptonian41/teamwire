//! A store, a registry of fakes and a clock, ready to hand to the locator.

use std::sync::Arc;

use teamwire_core::{
    Config, HarnessId, HostId, Liveness, Occupant, Pid, Place, PlaceState, Registry, SessionId,
    SessionKind, TeamName, Timestamp,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_store::{Ctx, NewTeam, Store};

pub(crate) struct Rig {
    pub(crate) home: TestHome,
    pub(crate) store: Store,
    pub(crate) registry: Registry,
    pub(crate) platform: FakePlatform,
    pub(crate) clock: FakeClock,
    pub(crate) config: Config,
    pub(crate) harness: Arc<FakeHarness>,
    pub(crate) host: Arc<FakeHost>,
}

pub(crate) fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

pub(crate) fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

impl Rig {
    /// A team whose lead runs session `lead-1` (interactive) on the fake
    /// harness and host.
    pub(crate) fn new() -> Self {
        Self::with_lead_kind(SessionKind::Interactive)
    }

    pub(crate) fn with_lead_kind(kind: SessionKind) -> Self {
        let harness = Arc::new(FakeHarness::new());
        let host = Arc::new(FakeHost::new());
        let mut registry = Registry::new();
        registry.add_harness(harness.clone()).unwrap();
        registry.add_host(host.clone()).unwrap();
        let home = TestHome::new().unwrap();
        let mut store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at(0),
                    &NewTeam {
                        name: TeamName::parse("alpha").unwrap(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("fake"),
                        lead_host: HostId::from_static("fake"),
                        lead_route: teamwire_core::LeadRoute::Sink,
                        lead_session: Some((sid("lead-1"), kind)),
                    },
                )?;
                Ok(())
            })
            .unwrap();
        Self {
            home,
            store,
            registry,
            platform: FakePlatform::new(),
            clock: FakeClock::new(),
            config: Config::default(),
            harness,
            host,
        }
    }

    pub(crate) fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        }
    }

    /// A live process that names `place_id` as its own place.
    pub(crate) fn claim(&self, pid: u32, place_id: &str) {
        self.platform.set_alive(Pid::new(pid), true);
        self.platform
            .set_env(Pid::new(pid), "FAKE_PLACE_ID", place_id);
    }

    /// One place that holds `session` for the host, run by process `pid`,
    /// which names that place for itself: the state of a healthy session.
    pub(crate) fn healthy(&self, session: &str, pid: u32) -> Place {
        let session = sid(session);
        let place = self.host.add_place(
            &format!("pane-{pid}"),
            PlaceState::Idle,
            Occupant {
                session: Some(session.clone()),
                pids: vec![],
            },
        );
        self.harness.add_session(
            session,
            vec![Pid::new(pid)],
            Liveness::Alive(PlaceState::Idle),
        );
        self.claim(pid, &place.id);
        self.host.set_screen(&place, "ready\n> ");
        place
    }
}
