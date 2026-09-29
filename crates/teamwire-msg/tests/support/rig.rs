//! A team on the fakes: a store, a registry, a clock and members that can be
//! typed to. Shared by the message tests; each file uses part of it.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::sync::Arc;

use serde_json::json;
use sha2::{Digest, Sha256};
use teamwire_core::{
    Actor, Clock as _, Config, DisplayId, HarnessId, HostId, LeadRoute, Liveness, MemberKind,
    MemberState, MessageType, Mode, NoHost, Occupant, Pid, Place, PlaceState, Recipient, Registry,
    Role, SendRequest, SessionId, SessionKind, TeamName, Timestamp,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_store::{Ctx, NewMember, NewTeam, Store};

pub(crate) struct Rig {
    pub(crate) home: TestHome,
    pub(crate) store: Store,
    pub(crate) registry: Registry,
    pub(crate) platform: FakePlatform,
    pub(crate) clock: Arc<FakeClock>,
    pub(crate) config: Config,
    pub(crate) harness: Arc<FakeHarness>,
    pub(crate) host: Arc<FakeHost>,
    team: TeamName,
    next_pid: u32,
}

pub(crate) fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

pub(crate) fn role(text: &str) -> Role {
    Role::parse(text).unwrap()
}

pub(crate) fn to(text: &str) -> Recipient {
    Recipient::parse(text).unwrap()
}

pub(crate) fn id(text: &str) -> DisplayId {
    DisplayId::parse(text).unwrap()
}

pub(crate) fn hash_of(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// The token a rig member holds.
pub(crate) fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

impl Rig {
    /// Team `alpha`, lead session `lead-1` (interactive) on the fake harness
    /// and host; `none` is registered too, for members without a host.
    pub(crate) fn new() -> Self {
        Self::with_lead_host("fake")
    }

    /// The lead's host has the id `lead_host`; `none` is a lead with no host.
    pub(crate) fn with_lead_host(lead_host: &'static str) -> Self {
        let harness = Arc::new(FakeHarness::new());
        let host = Arc::new(FakeHost::new());
        let mut registry = Registry::new();
        registry.add_harness(harness.clone()).unwrap();
        registry.add_host(host.clone()).unwrap();
        registry.add_host(Arc::new(NoHost)).unwrap();
        let home = TestHome::new().unwrap();
        let clock = Arc::new(FakeClock::new());
        let mut store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    clock.now(),
                    &NewTeam {
                        name: TeamName::parse("alpha").unwrap(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("fake"),
                        lead_host: HostId::from_static(lead_host),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((sid("lead-1"), SessionKind::Interactive)),
                    },
                )?;
                Ok(())
            })
            .unwrap();
        harness.set_interactive_caller(sid("lead-1"));
        let mut rig = Self {
            home,
            store,
            registry,
            platform: FakePlatform::new(),
            clock,
            config: Config::default(),
            harness,
            host,
            team: TeamName::parse("alpha").unwrap(),
            next_pid: 100,
        };
        rig.healthy_lead();
        rig
    }

    pub(crate) fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &*self.clock,
            config: &self.config,
        }
    }

    pub(crate) fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// One place that holds `session`, run by a process that names that
    /// place for itself: the state of a healthy session.
    fn healthy(&mut self, session: &str) -> Place {
        self.next_pid += 1;
        let pid = self.next_pid;
        let place = self.host.add_place(
            &format!("pane-{pid}"),
            PlaceState::Idle,
            Occupant {
                session: Some(sid(session)),
                pids: vec![],
            },
        );
        self.harness.add_session(
            sid(session),
            vec![Pid::new(pid)],
            Liveness::Alive(PlaceState::Idle),
        );
        self.platform.set_alive(Pid::new(pid), true);
        self.platform
            .set_env(Pid::new(pid), "FAKE_PLACE_ID", &place.id);
        self.host.set_screen(&place, "ready\n> ");
        place
    }

    pub(crate) fn healthy_lead(&mut self) -> Place {
        self.healthy("lead-1")
    }

    /// A started member on the fake host with a healthy session and a token.
    pub(crate) fn add_member(&mut self, name: &str, kind: MemberKind) -> Place {
        self.add_member_on(name, kind, "fake")
    }

    /// A started member on host `host_id`. A member on `none` has no place.
    pub(crate) fn add_member_on(
        &mut self,
        name: &str,
        kind: MemberKind,
        host_id: &'static str,
    ) -> Place {
        let r = role(name);
        let session = format!("session-{name}");
        let hash = hash_of(&token_of(name));
        let at = self.now();
        let cwd = self.home.path().join("work").join(name);
        self.store
            .write(|tx| {
                tx.insert_member(
                    at,
                    &NewMember {
                        role: r.clone(),
                        title: name.to_owned(),
                        kind,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("fake"),
                        host: HostId::from_static(host_id),
                        cwd,
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_session(&r, &sid(&session))?;
                tx.set_member_token_hash(&r, &hash)?;
                tx.set_member_state(at, &r, MemberState::Started, &json!({}))
            })
            .unwrap();
        if host_id == "fake" {
            self.healthy(&session)
        } else {
            Place {
                host: HostId::from_static(host_id),
                id: "none".to_owned(),
            }
        }
    }

    /// A request from `from` to `targets`, body `text`.
    pub(crate) fn request(
        &self,
        from: &str,
        targets: &[&str],
        kind: MessageType,
        text: &str,
    ) -> SendRequest {
        SendRequest {
            team: self.team.clone(),
            actor: Actor::parse(from).unwrap(),
            to: targets.iter().map(|t| to(t)).collect(),
            kind,
            body: text.to_owned(),
            commit: None,
            re: None,
            client_key: None,
        }
    }

    /// Every event of `kind`, oldest first.
    pub(crate) fn events(&self, kind: teamwire_core::EventType) -> Vec<teamwire_core::Event> {
        self.store
            .read(|tx| tx.events_of_type(kind)?.all_or_corrupt())
            .unwrap()
    }
}
