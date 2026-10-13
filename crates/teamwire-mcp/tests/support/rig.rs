//! Team `alpha` on the fakes and an `rmcp` client for it: shared by the
//! MCP integration tests; each file uses part of it.
#![expect(
    clippy::unwrap_used,
    reason = "an integration test unwraps; a failure there fails the test"
)]

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use teamwire_core::{
    Actor, Clock as _, Config, EventType, HarnessId, HostId, LeadRoute, Liveness, MemberKind,
    MemberState, Mode, NoHost, Occupant, Pid, PlaceState, Registry, Role, SessionId, SessionKind,
    TeamName,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_mcp::{McpCaller, McpServer, ServerEnv};
use teamwire_msg::{LogFilter, token_hash};
use teamwire_store::{Ctx, NewMember, NewTeam, Store};

pub(crate) type Client = RunningService<RoleClient, ()>;

pub(crate) fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

pub(crate) fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

/// Team `alpha` on the fakes, with a lead session `lead-1` and started
/// members that can be typed to.
pub(crate) struct Rig {
    pub(crate) home: TestHome,
    pub(crate) store: Store,
    pub(crate) registry: Arc<Registry>,
    pub(crate) platform: Arc<FakePlatform>,
    pub(crate) clock: Arc<FakeClock>,
    pub(crate) harness: Arc<FakeHarness>,
    pub(crate) host: Arc<FakeHost>,
    pub(crate) next_pid: u32,
    pub(crate) config: Config,
}

impl Rig {
    pub(crate) fn new() -> Self {
        let harness = Arc::new(FakeHarness::new());
        let host = Arc::new(FakeHost::new());
        let mut registry = Registry::new();
        registry.add_harness(harness.clone()).unwrap();
        registry.add_host(host.clone()).unwrap();
        registry.add_host(Arc::new(NoHost)).unwrap();
        let home = TestHome::new().unwrap();
        let clock = Arc::new(FakeClock::new());
        let team = TeamName::parse("alpha").unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    clock.now(),
                    &NewTeam {
                        name: team.clone(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("fake"),
                        lead_host: HostId::from_static("fake"),
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
            registry: Arc::new(registry),
            platform: Arc::new(FakePlatform::new()),
            clock,
            harness,
            host,
            next_pid: 100,
            config: Config::default(),
        };
        rig.healthy("lead-1");
        rig
    }

    /// A place that holds `session`, run by a process that names that place.
    pub(crate) fn healthy(&mut self, session: &str) {
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
    }

    pub(crate) fn add_member(&mut self, name: &str, kind: MemberKind) {
        let role = Role::parse(name).unwrap();
        let session = format!("session-{name}");
        let at = self.clock.now();
        let cwd = self.home.path().join("work").join(name);
        self.store
            .write(|tx| {
                tx.insert_member(
                    at,
                    &NewMember {
                        role: role.clone(),
                        title: name.to_owned(),
                        kind,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("fake"),
                        host: HostId::from_static("fake"),
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
                tx.set_member_session(&role, &sid(&session))?;
                tx.set_member_token_hash(&role, &token_hash(&token_of(name)))?;
                tx.set_member_state(at, &role, MemberState::Started, &json!({}))
            })
            .unwrap();
        self.healthy(&session);
    }

    /// The session's process is gone (`false`) or back (`true`).
    pub(crate) fn set_reachable(&self, member: &str, pid: u32, reachable: bool) {
        let (pids, liveness) = if reachable {
            (vec![Pid::new(pid)], Liveness::Alive(PlaceState::Idle))
        } else {
            (vec![], Liveness::Dead)
        };
        self.harness
            .add_session(sid(&format!("session-{member}")), pids, liveness);
    }

    pub(crate) fn env(&self) -> ServerEnv {
        ServerEnv {
            home: self.home.path().to_path_buf(),
            registry: Arc::clone(&self.registry),
            platform: self.platform.clone(),
            clock: self.clock.clone(),
            config: Config::default(),
        }
    }

    pub(crate) fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &*self.platform,
            clock: &*self.clock,
            config: &self.config,
        }
    }

    pub(crate) async fn member_client(&self, name: &str, token: &str) -> Client {
        self.client(McpCaller::Member {
            team: TeamName::parse("alpha").unwrap(),
            role: Role::parse(name).unwrap(),
            token: token.to_owned(),
        })
        .await
    }

    pub(crate) async fn client(&self, caller: McpCaller) -> Client {
        let (server_side, client_side) = tokio::io::duplex(1 << 20);
        let server = McpServer::new(self.env(), caller);
        tokio::spawn(async move {
            if let Ok(running) = server.serve(server_side).await {
                running.waiting().await.ok();
            }
        });
        ().serve(client_side).await.unwrap()
    }

    /// Every stored event's type, and how many messages the log holds.
    pub(crate) fn stored(&mut self) -> (Vec<EventType>, usize) {
        let events = self
            .store
            .read(|tx| Ok(tx.events_after(0, None, None)?.items))
            .unwrap()
            .into_iter()
            .map(|e| e.kind)
            .collect();
        let messages = teamwire_msg::log(
            &self.ctx(),
            LogFilter {
                viewer: Actor::Lead,
                role: None,
                limit: None,
            },
        )
        .unwrap()
        .len();
        (events, messages)
    }
}

pub(crate) fn arguments(value: &Value) -> serde_json::Map<String, Value> {
    value.as_object().cloned().unwrap()
}

pub(crate) async fn call(client: &Client, tool: &str, args: Value) -> CallToolResult {
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments(&args)))
        .await
        .unwrap()
}

pub(crate) fn ok(result: &CallToolResult) -> &Value {
    assert_ne!(result.is_error, Some(true), "refused: {result:?}");
    result.structured_content.as_ref().unwrap()
}

/// The error code of a refused call, checking the whole error shape.
pub(crate) fn refused(result: &CallToolResult) -> String {
    assert_eq!(result.is_error, Some(true), "not refused: {result:?}");
    let error = &result.structured_content.as_ref().unwrap()["error"];
    assert!(error["message"].is_string());
    assert!(error["retry"].is_boolean());
    error["code"].as_str().unwrap().to_owned()
}

pub(crate) async fn two_members() -> (Rig, Client, Client) {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Investigator);
    rig.add_member("web", MemberKind::Implementer);
    let api = rig.member_client("api", &token_of("api")).await;
    let web = rig.member_client("web", &token_of("web")).await;
    (rig, api, web)
}
