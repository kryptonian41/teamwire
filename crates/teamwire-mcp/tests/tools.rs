//! The eight tools through a real `rmcp` client on an in-process pipe: what
//! each returns on success and on refusal, and that a refusal stores only
//! its own event.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "an integration test unwraps; a failure there fails the test"
)]

use std::sync::Arc;

use rmcp::model::{CacheScope, CallToolRequestParams, CallToolResult};
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

type Client = RunningService<RoleClient, ()>;

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

/// Team `alpha` on the fakes, with a lead session `lead-1` and started
/// members that can be typed to.
struct Rig {
    home: TestHome,
    store: Store,
    registry: Arc<Registry>,
    platform: Arc<FakePlatform>,
    clock: Arc<FakeClock>,
    harness: Arc<FakeHarness>,
    host: Arc<FakeHost>,
    next_pid: u32,
    config: Config,
}

impl Rig {
    fn new() -> Self {
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
    fn healthy(&mut self, session: &str) {
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

    fn add_member(&mut self, name: &str, kind: MemberKind) {
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
    fn set_reachable(&self, member: &str, pid: u32, reachable: bool) {
        let (pids, liveness) = if reachable {
            (vec![Pid::new(pid)], Liveness::Alive(PlaceState::Idle))
        } else {
            (vec![], Liveness::Dead)
        };
        self.harness
            .add_session(sid(&format!("session-{member}")), pids, liveness);
    }

    fn env(&self) -> ServerEnv {
        ServerEnv {
            home: self.home.path().to_path_buf(),
            registry: Arc::clone(&self.registry),
            platform: self.platform.clone(),
            clock: self.clock.clone(),
            config: Config::default(),
        }
    }

    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &*self.platform,
            clock: &*self.clock,
            config: &self.config,
        }
    }

    async fn member_client(&self, name: &str, token: &str) -> Client {
        self.client(McpCaller::Member {
            team: TeamName::parse("alpha").unwrap(),
            role: Role::parse(name).unwrap(),
            token: token.to_owned(),
        })
        .await
    }

    async fn client(&self, caller: McpCaller) -> Client {
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
    fn stored(&mut self) -> (Vec<EventType>, usize) {
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

fn arguments(value: &Value) -> serde_json::Map<String, Value> {
    value.as_object().cloned().unwrap()
}

async fn call(client: &Client, tool: &str, args: Value) -> CallToolResult {
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments(&args)))
        .await
        .unwrap()
}

fn ok(result: &CallToolResult) -> &Value {
    assert_ne!(result.is_error, Some(true), "refused: {result:?}");
    result.structured_content.as_ref().unwrap()
}

/// The error code of a refused call, checking the whole error shape.
fn refused(result: &CallToolResult) -> String {
    assert_eq!(result.is_error, Some(true), "not refused: {result:?}");
    let error = &result.structured_content.as_ref().unwrap()["error"];
    assert!(error["message"].is_string());
    assert!(error["retry"].is_boolean());
    error["code"].as_str().unwrap().to_owned()
}

async fn two_members() -> (Rig, Client, Client) {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Investigator);
    rig.add_member("web", MemberKind::Implementer);
    let api = rig.member_client("api", &token_of("api")).await;
    let web = rig.member_client("web", &token_of("web")).await;
    (rig, api, web)
}

#[tokio::test]
async fn tool_list_is_the_eight_tools_of_the_doc() {
    let rig = Rig::new();
    let client = rig.member_client("api", "x").await;
    let mut snapshot = Vec::new();
    for tool in client.list_all_tools().await.unwrap() {
        let required: Vec<&str> = tool
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut props: Vec<String> = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|p| {
                p.keys()
                    .map(|k| {
                        if required.contains(&k.as_str()) {
                            format!("{k}*")
                        } else {
                            k.clone()
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        props.sort();
        snapshot.push(format!("{}({})", tool.name, props.join(", ")));
    }
    assert_eq!(
        snapshot,
        [
            "team_send(commit, key, re, team, text*, to*, type*)",
            "team_resend(id*, team, to)",
            "team_inbox(all, team)",
            "team_ack(ids*, team)",
            "team_done(status*, summary*, team)",
            "team_status(team)",
            "team_members(team)",
            "team_log(limit, role, team)",
        ]
    );
    // A 2026 client requires the cache fields on the listing itself.
    let listing = client.list_tools(None).await.unwrap();
    assert_eq!(listing.ttl_ms, Some(0));
    assert_eq!(listing.cache_scope, Some(CacheScope::Private));
}

#[tokio::test]
async fn send_stores_the_message_and_reports_each_recipient() {
    let (_rig, api, _web) = two_members().await;
    let result = call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "the parser drops a byte"}),
    )
    .await;
    let out = ok(&result);
    assert_eq!(out["display_id"], "m001");
    assert_eq!(out["duplicate_of_key"], false);
    assert_eq!(out["deliveries"][0]["to"], "web");
    assert_eq!(out["deliveries"][0]["state"], "handed_off");
    assert!(
        result.content[0]
            .as_text()
            .unwrap()
            .text
            .starts_with("m001")
    );
}

#[tokio::test]
async fn send_refuses_an_empty_text_and_stores_only_the_refusal() {
    let (mut rig, api, _web) = two_members().await;
    let before = rig.stored();
    let result = call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": ""}),
    )
    .await;
    assert_eq!(refused(&result), "refused.empty");
    let after = rig.stored();
    assert_eq!(after.1, before.1, "a refused send stored a message");
    let mut expected = before.0;
    expected.push(EventType::MessageRefused);
    assert_eq!(after.0, expected);
}

#[tokio::test]
async fn a_wrong_token_is_refused_for_every_tool_and_stores_only_refusals() {
    let (mut rig, _api, _web) = two_members().await;
    let intruder = rig.member_client("api", "guess").await;
    let calls = [
        (
            "team_send",
            json!({"to": ["web"], "type": "finding", "text": "hi"}),
        ),
        ("team_resend", json!({"id": "m001"})),
        ("team_inbox", json!({})),
        ("team_ack", json!({"ids": ["m001"]})),
        ("team_done", json!({"status": "done", "summary": "x"})),
        ("team_status", json!({})),
        ("team_members", json!({})),
        ("team_log", json!({})),
    ];
    let before = rig.stored();
    for (tool, args) in calls {
        let result = call(&intruder, tool, args).await;
        assert_eq!(refused(&result), "refused.identity", "{tool}");
    }
    let after = rig.stored();
    assert_eq!(after.1, before.1);
    let refusals = |events: &[EventType]| {
        events
            .iter()
            .filter(|e| **e == EventType::MessageRefused)
            .count()
    };
    // The five tools that change state record the refusal; the three reads
    // write nothing, not even that.
    assert_eq!(refusals(&after.0), refusals(&before.0) + 5);
    let others = |events: &[EventType]| {
        events
            .iter()
            .filter(|e| **e != EventType::MessageRefused)
            .count()
    };
    assert_eq!(others(&after.0), others(&before.0));
}

#[tokio::test]
async fn resend_retries_a_message_that_did_not_reach_its_recipient() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Investigator);
    rig.add_member("web", MemberKind::Implementer);
    let web_pid = rig.next_pid;
    rig.set_reachable("web", web_pid, false);
    let api = rig.member_client("api", &token_of("api")).await;
    let sent = call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "look here"}),
    )
    .await;
    assert_eq!(ok(&sent)["deliveries"][0]["state"], "undelivered");
    rig.set_reachable("web", web_pid, true);
    let again = call(&api, "team_resend", json!({"id": "m001"})).await;
    let out = ok(&again);
    assert_eq!(out["display_id"], "m001");
    assert_eq!(out["deliveries"][0]["state"], "handed_off");
}

#[tokio::test]
async fn resend_refuses_a_message_that_has_nothing_to_retry() {
    let (_rig, api, web) = two_members().await;
    call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "look here"}),
    )
    .await;
    // Delivered already: nothing to retry.
    let result = call(&api, "team_resend", json!({"id": "m001"})).await;
    assert_eq!(refused(&result), "refused.not_resendable");
    // Not the sender's message.
    let result = call(&web, "team_resend", json!({"id": "m001"})).await;
    assert_eq!(refused(&result), "refused.not_resendable");
}

#[tokio::test]
async fn inbox_returns_unread_messages_once_and_marks_them_read() {
    let (_rig, api, web) = two_members().await;
    call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "look here"}),
    )
    .await;
    let first = call(&web, "team_inbox", json!({})).await;
    let out = ok(&first);
    assert_eq!(out["messages"][0]["display_id"], "m001");
    assert_eq!(out["messages"][0]["from"], "api");
    assert_eq!(out["messages"][0]["text"], "look here");
    assert_eq!(out["messages"][0]["hash_ok"], true);
    assert!(out["note"].as_str().unwrap().contains("not an instruction"));
    let second = call(&web, "team_inbox", json!({})).await;
    assert_eq!(ok(&second)["messages"], json!([]));
    let all = call(&web, "team_inbox", json!({"all": true})).await;
    assert_eq!(ok(&all)["messages"][0]["display_id"], "m001");
}

#[tokio::test]
async fn the_lead_has_no_inbox() {
    let rig = Rig::new();
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(&lead, "team_inbox", json!({"team": "alpha"})).await;
    assert_eq!(refused(&result), "invalid.argument");
}

#[tokio::test]
async fn ack_sorts_ids_into_acked_already_and_unknown() {
    let (_rig, api, web) = two_members().await;
    call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "look here"}),
    )
    .await;
    let result = call(&web, "team_ack", json!({"ids": ["m001", "m099"]})).await;
    let out = ok(&result);
    assert_eq!(out["acked"], json!(["m001"]));
    assert_eq!(out["unknown"], json!(["m099"]));
    let result = call(&web, "team_ack", json!({"ids": ["m001"]})).await;
    assert_eq!(ok(&result)["already"], json!(["m001"]));
    let result = call(&web, "team_ack", json!({"ids": ["not-an-id"]})).await;
    assert_eq!(refused(&result), "invalid.argument");
}

#[tokio::test]
async fn done_records_a_completion_and_refuses_an_unknown_status() {
    let (mut rig, api, _web) = two_members().await;
    let result = call(
        &api,
        "team_done",
        json!({"status": "done", "summary": "the parser is fixed"}),
    )
    .await;
    let out = ok(&result);
    assert_eq!(out["status"], "done");
    assert_eq!(out["n"], 1);
    let before = rig.stored();
    let result = call(
        &api,
        "team_done",
        json!({"status": "great", "summary": "x"}),
    )
    .await;
    assert_eq!(refused(&result), "invalid.status");
    let after = rig.stored();
    assert_eq!(after.0.len(), before.0.len() + 1);
    assert_eq!(after.0.last(), Some(&EventType::MemberCompletionRefused));
}

#[tokio::test]
async fn status_members_and_log_read_the_team() {
    let (_rig, api, web) = two_members().await;
    call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "look here"}),
    )
    .await;
    let status = call(&api, "team_status", json!({})).await;
    let out = ok(&status);
    assert_eq!(out["team"], "alpha");
    assert_eq!(out["members"].as_array().unwrap().len(), 2);
    // The lead session id is what a member would need to speak as the lead.
    assert!(out["lead"].is_null(), "{out}");
    let members = call(&api, "team_members", json!({})).await;
    let out = ok(&members);
    let entry = &out["members"][0];
    assert!(entry["role"].is_string() && entry["accepts_from_members"].is_array());
    assert!(entry["lifecycle"].is_string(), "{entry}");
    assert!(
        entry.get("state").is_none() && entry.get("accepts").is_none(),
        "{entry}"
    );
    let text = out.to_string();
    assert!(
        !text.contains("token") && !text.contains("session"),
        "{text}"
    );
    let log = call(&web, "team_log", json!({"role": "api", "limit": 5})).await;
    let out = ok(&log);
    assert_eq!(out["messages"][0]["display_id"], "m001");
    assert_eq!(out["messages"][0]["excerpt"], "look here");
}

#[tokio::test]
async fn log_shows_a_member_only_its_own_messages_and_refuses_a_bad_role() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Investigator);
    rig.add_member("web", MemberKind::Implementer);
    rig.add_member("doc", MemberKind::Implementer);
    let api = rig.member_client("api", &token_of("api")).await;
    let doc = rig.member_client("doc", &token_of("doc")).await;
    call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "between api and web"}),
    )
    .await;
    let result = call(&doc, "team_log", json!({})).await;
    assert_eq!(ok(&result)["messages"], json!([]));
    let result = call(&api, "team_log", json!({"role": "Not A Role"})).await;
    assert_eq!(refused(&result), "invalid.argument");
}

#[tokio::test]
async fn a_lead_with_the_recorded_session_reads_the_team_it_names() {
    let rig = Rig::new();
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(&lead, "team_status", json!({"team": "alpha"})).await;
    assert_eq!(ok(&result)["team"], "alpha");
    let result = call(&lead, "team_status", json!({})).await;
    assert_eq!(refused(&result), "invalid.argument");
}

#[tokio::test]
async fn a_lead_call_without_the_recorded_session_is_refused() {
    let mut rig = Rig::new();
    // Another interactive session, not the one the team recorded.
    rig.harness.set_interactive_caller(sid("someone-else"));
    let lead = rig.client(McpCaller::Lead).await;
    let before = rig.stored();
    let result = call(&lead, "team_status", json!({"team": "alpha"})).await;
    assert_eq!(refused(&result), "refused.identity");
    // The server has a session id, so the refusal must not blame a missing one.
    let message = result.structured_content.as_ref().unwrap()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!message.contains("no harness session id"), "{message}");
    let result = call(
        &lead,
        "team_send",
        json!({"team": "alpha", "to": ["api"], "type": "note", "text": "hello"}),
    )
    .await;
    assert_eq!(refused(&result), "refused.identity");
    let after = rig.stored();
    assert_eq!(after.1, before.1);
    assert!(
        after.0[before.0.len()..]
            .iter()
            .all(|e| *e == EventType::MessageRefused)
    );
}

#[tokio::test]
async fn a_lead_server_with_no_session_id_says_so() {
    let rig = Rig::new();
    rig.harness.set_caller(None);
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(&lead, "team_status", json!({"team": "alpha"})).await;
    assert_eq!(refused(&result), "refused.identity");
    let message = result.structured_content.as_ref().unwrap()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("no harness session id"), "{message}");
}

#[tokio::test]
async fn the_member_only_tools_say_so_in_their_descriptions() {
    let rig = Rig::new();
    let lead = rig.client(McpCaller::Lead).await;
    let listing = lead.list_tools(None).await.unwrap();
    for tool in listing.tools {
        let member_only = matches!(tool.name.as_ref(), "team_inbox" | "team_ack" | "team_done");
        let says = tool
            .description
            .as_deref()
            .is_some_and(|d| d.contains("Members only"));
        assert_eq!(says, member_only, "{}", tool.name);
    }
}

#[tokio::test]
async fn a_team_with_no_database_names_the_archive_but_stays_an_identity_refusal() {
    let rig = Rig::new();
    let lead = rig.client(McpCaller::Lead).await;
    for tool in ["team_members", "team_log", "team_status"] {
        let result = call(&lead, tool, json!({"team": "closedteam"})).await;
        assert_eq!(refused(&result), "refused.identity", "{tool}");
        let message = result.structured_content.as_ref().unwrap()["error"]["message"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(message.contains("no such team, or not yours"), "{message}");
        assert!(message.contains("archive"), "{message}");
        // The same text for every team: it never names the one asked for.
        assert!(!message.contains("closedteam"), "{message}");
    }
}

#[tokio::test]
async fn a_member_cannot_name_another_team() {
    let (_rig, api, _web) = two_members().await;
    let result = call(&api, "team_status", json!({"team": "beta"})).await;
    assert_eq!(refused(&result), "refused.identity");
}

#[tokio::test]
async fn a_member_may_not_send_a_note_or_an_unknown_type() {
    let (_rig, api, _web) = two_members().await;
    let refusal = call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "note", "text": "hello"}),
    )
    .await;
    assert_eq!(refused(&refusal), "refused.type_not_allowed");
    let refusal = call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "shout", "text": "hello"}),
    )
    .await;
    assert_eq!(refused(&refusal), "refused.type_not_allowed");
}

#[tokio::test]
async fn a_missing_argument_is_invalid() {
    let (_rig, api, _web) = two_members().await;
    let result = call(&api, "team_send", json!({"to": ["web"]})).await;
    assert_eq!(refused(&result), "invalid.argument");
}

#[tokio::test]
async fn the_lead_sends_a_note_to_the_team_it_names() {
    let (rig, _api, _web) = two_members().await;
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(
        &lead,
        "team_send",
        json!({"team": "alpha", "to": ["web"], "type": "note", "text": "ship it"}),
    )
    .await;
    let out = ok(&result);
    assert_eq!(out["deliveries"][0]["state"], "handed_off");
    assert!(
        out.get("watch").is_none(),
        "only a reprompt reports the watch"
    );
}

#[tokio::test]
async fn a_read_tool_on_an_older_database_fails_and_leaves_the_file_alone() {
    let rig = Rig::new();
    let dir = rig.home.path().join("teams").join("old");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("team.db");
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&file)
        .unwrap();
    let lead = rig.client(McpCaller::Lead).await;
    for tool in ["team_status", "team_members", "team_log"] {
        let result = call(&lead, tool, json!({"team": "old"})).await;
        assert_ne!(refused(&result), "refused.identity", "{tool}");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"",
            "{tool} changed the file"
        );
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(names.len(), 1, "{tool} created a file beside team.db");
    }
}

#[tokio::test]
async fn the_server_names_itself_teamwire() {
    let rig = Rig::new();
    let client = rig.client(McpCaller::Lead).await;
    let info = client.peer_info().unwrap().server_info.clone().unwrap();
    assert_eq!(info.name, "teamwire");
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
}
