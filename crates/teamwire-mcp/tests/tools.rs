//! The ten tools through a real `rmcp` client on an in-process pipe: what
//! each returns on success and on refusal, and that a refusal stores only
//! its own event.
#![expect(
    clippy::indexing_slicing,
    reason = "a test indexes the JSON it checks; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use rmcp::model::{CacheScope, CallToolResult};
use serde_json::{Value, json};
use teamwire_core::{EventType, MemberKind, Platform, Role, TurnEnd, TurnEndReply};
use teamwire_mcp::{McpCaller, ServerEnv};
use teamwire_msg::CallerEnv;

use rig::{Rig, call, ok, refused, sid, token_of, two_members};

#[tokio::test]
async fn tool_list_is_the_ten_tools_of_the_doc() {
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
            "team_members(of, team)",
            "team_log(limit, role, team)",
            "team_teams(team)",
            "team_decisions(grep, limit, of, ref, role, team)",
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
        ("team_decisions", json!({})),
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
    // The five tools that change state record the refusal; the four reads
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
async fn the_inbox_note_is_left_out_when_only_the_lead_wrote() {
    let (rig, _api, web) = two_members().await;
    let lead = rig.client(McpCaller::Lead).await;
    call(
        &lead,
        "team_send",
        json!({"team": "alpha", "to": ["web"], "type": "note", "text": "ship it"}),
    )
    .await;
    let result = call(&web, "team_inbox", json!({})).await;
    let out = ok(&result);
    assert_eq!(out["messages"][0]["from"], "lead");
    assert!(out.get("note").is_none(), "{out}");
}

#[tokio::test]
async fn the_lead_has_no_inbox() {
    let rig = Rig::new();
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(&lead, "team_inbox", json!({"team": "alpha"})).await;
    assert_eq!(refused(&result), "invalid.argument");
}

#[tokio::test]
async fn log_accepts_the_lead_and_another_teams_agent_as_the_role() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Investigator);
    let api = rig.member_client("api", &token_of("api")).await;
    for role in ["lead", "db@beta"] {
        let result = call(&api, "team_log", json!({"role": role})).await;
        assert_eq!(ok(&result)["messages"], json!([]), "{role}");
    }
}

#[tokio::test]
async fn log_reads_an_address_that_names_the_own_team_as_the_local_one() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Investigator);
    rig.add_member("web", MemberKind::Implementer);
    let api = rig.member_client("api", &token_of("api")).await;
    call(
        &api,
        "team_send",
        json!({"to": ["web"], "type": "finding", "text": "between api and web"}),
    )
    .await;
    for role in ["web", "web@alpha"] {
        let result = call(&api, "team_log", json!({"role": role})).await;
        assert_eq!(ok(&result)["messages"][0]["display_id"], "m001", "{role}");
    }
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
async fn a_lead_server_started_in_a_member_session_refuses_and_logs_it() {
    let (mut rig, _api, _web) = two_members().await;
    // With the member's own token, and with only its team and role: either
    // way the environment is a member's, so it proves no lead (ADR 0008).
    let with_token = CallerEnv {
        team: Some("alpha".to_owned()),
        role: Some("api".to_owned()),
        token: Some(token_of("api")),
        ..CallerEnv::default()
    };
    let without_token = CallerEnv {
        token: None,
        ..with_token.clone()
    };
    for process in [with_token, without_token] {
        let env = ServerEnv {
            process: process.clone(),
            ..rig.env()
        };
        let lead = rig.client_with(env, McpCaller::Lead).await;
        let before = rig.stored();
        let result = call(
            &lead,
            "team_send",
            json!({"team": "alpha", "to": ["web"], "type": "note", "text": "ship it"}),
        )
        .await;
        assert_eq!(refused(&result), "refused.identity", "{process:?}");
        let after = rig.stored();
        assert_eq!(after.1, before.1, "nothing is sent: {process:?}");
        assert_eq!(
            after.0[before.0.len()..],
            [EventType::MessageRefused],
            "{process:?}"
        );
    }
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
async fn the_send_tool_states_the_message_limits() {
    let rig = Rig::new();
    let lead = rig.client(McpCaller::Lead).await;
    let listing = lead.list_tools(None).await.unwrap();
    let send = listing
        .tools
        .iter()
        .find(|t| t.name == "team_send")
        .unwrap();
    let text = send.input_schema["properties"]["text"]["description"]
        .as_str()
        .unwrap();
    assert!(text.contains("600") && text.contains("800"), "{text}");
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
    #[expect(
        clippy::disallowed_methods,
        reason = "the test makes an empty file, not a store file"
    )]
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

#[tokio::test]
async fn no_tool_call_speaks_as_the_user_even_on_a_server_that_holds_the_user_key() {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    #[expect(
        clippy::disallowed_methods,
        reason = "the test writes the key file `teamwire user-key` would have written"
    )]
    std::fs::write(rig.home.path().join("user.key"), "the-key\n").unwrap();
    let holds_key = || ServerEnv {
        process: CallerEnv {
            from: Some("user".to_owned()),
            user_key: Some("the-key".to_owned()),
            ..CallerEnv::default()
        },
        ..rig.env()
    };
    let note =
        json!({"team": "alpha", "to": ["api"], "type": "note", "text": "hi", "from": "user"});
    // The lead's claim stays the lead's: with another session it is refused.
    rig.harness.set_interactive_caller(sid("someone-else"));
    let lead = rig.client_with(holds_key(), McpCaller::Lead).await;
    assert_eq!(
        refused(&call(&lead, "team_send", note.clone()).await),
        "refused.identity"
    );
    // With the recorded session the note is the lead's, not the user's.
    rig.harness.set_interactive_caller(sid("lead-1"));
    let lead = rig.client_with(holds_key(), McpCaller::Lead).await;
    ok(&call(&lead, "team_send", note).await);
    // A member's call is the member's.
    let api = rig
        .client_with(
            holds_key(),
            McpCaller::Member {
                team: teamwire_core::TeamName::parse("alpha").unwrap(),
                role: teamwire_core::Role::parse("api").unwrap(),
                token: token_of("api"),
            },
        )
        .await;
    ok(&call(
        &api,
        "team_send",
        json!({"to": ["lead"], "type": "finding", "text": "from api", "from": "user"}),
    )
    .await);
    let senders: Vec<String> = rig
        .store
        .read(|tx| tx.messages(&teamwire_store::MessageFilter::default()))
        .unwrap()
        .items
        .into_iter()
        .map(|m| m.sender.to_string())
        .collect();
    assert_eq!(senders, ["lead", "api"]);
}

// ADR 0059: the text a model reads from a tool call names what it needs to
// act on: each member's role, state and generation, and for a refused send
// that nothing was sent. A model that reads only the text, as a lead did
// that took a refused send for a sent one, must not need the structured
// content to tell the two apart.

/// The text a model reads from a tool call.
fn text(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn members_and_status_name_each_role_its_state_and_generation_in_the_text() {
    let (rig, _api, _web) = two_members().await;
    let lead = rig.client(McpCaller::Lead).await;

    let members = call(&lead, "team_members", json!({"team": "alpha"})).await;
    ok(&members);
    assert_eq!(
        text(&members),
        "team alpha: 2 member(s): api (investigator, started, generation 1), \
         web (implementer, started, generation 1)"
    );

    let status = call(&lead, "team_status", json!({"team": "alpha"})).await;
    let view = ok(&status).clone();
    let said = text(&status);
    assert!(said.starts_with("team alpha: 2 member(s): api: "), "{said}");
    for m in view["members"].as_array().unwrap() {
        let part = format!(
            "{}: {}, generation {}",
            m["role"].as_str().unwrap(),
            m["state"].as_str().unwrap(),
            m["generation"]
        );
        assert!(said.contains(&part), "{part} not in {said}");
    }
}

#[tokio::test]
async fn a_reprompt_receipt_names_the_role_its_state_and_its_new_generation() {
    let (rig, _api, _web) = two_members().await;
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(
        &lead,
        "team_send",
        json!({"team": "alpha", "to": ["web"], "type": "reprompt", "text": "do part two"}),
    )
    .await;
    let out = ok(&result);
    assert_eq!(out["deliveries"][0]["to"], "web");
    assert_eq!(
        text(&result),
        "m001 reprompt sent: web: handed_off, now generation 2"
    );
}

#[tokio::test]
async fn a_refused_send_says_in_its_text_that_nothing_was_sent() {
    let (mut rig, _api, _web) = two_members().await;
    let lead = rig.client(McpCaller::Lead).await;
    let cases = [
        // The lead to itself, a word that is no role, and no recipient: the
        // three recipients a lead used once and read as sent.
        (json!(["lead"]), "refused.self"),
        (json!(["members"]), "refused.unknown_recipient"),
        (json!([]), "invalid.argument"),
    ];
    for (to, code) in cases {
        let before = rig.stored();
        let result = call(
            &lead,
            "team_send",
            json!({"team": "alpha", "to": to, "type": "reprompt", "text": "do part two"}),
        )
        .await;
        assert_eq!(refused(&result), code, "{to}");
        let said = text(&result);
        assert!(said.starts_with("NOT SENT: "), "{said}");
        assert!(said.contains(code), "{said}");
        assert!(
            said.contains(
                "No message was stored, no generation started, and there is no message id"
            ),
            "{said}"
        );
        let after = rig.stored();
        assert_eq!(after.1, before.1, "no message stored for {to}");
        // Each refusal is recorded, so `teamwire close` can tell that the
        // lead's newest send was refused.
        assert_eq!(
            after.0[before.0.len()..],
            [EventType::MessageRefused],
            "{to}"
        );
    }
    // A missing recipient field is refused the same way.
    let result = call(
        &lead,
        "team_send",
        json!({"team": "alpha", "type": "reprompt", "text": "do part two"}),
    )
    .await;
    assert_eq!(refused(&result), "invalid.argument");
    assert!(text(&result).starts_with("NOT SENT: "));
    let refusals = rig
        .store
        .read(|tx| tx.events_of_type(EventType::MessageRefused))
        .unwrap()
        .items;
    assert_eq!(refusals.len(), 4, "{refusals:?}");
    assert!(
        refusals.iter().all(|e| e.data["sender"] == "lead"),
        "{refusals:?}"
    );
    assert_eq!(refusals[3].data["code"], "invalid_argument");
    // No generation moved.
    let gens: Vec<u32> = rig
        .store
        .read(|tx| tx.members().map(|m| m.items))
        .unwrap()
        .iter()
        .map(|m| m.generation)
        .collect();
    assert_eq!(gens, [1, 1]);
}

/// The member side through MCP alone: the reprompt is read with `team_inbox`
/// and the result recorded with `team_done`, and the stop then delivers that
/// result with no corrective hold. No shell command is run.
#[tokio::test]
async fn a_member_reads_with_team_inbox_and_finishes_with_team_done() {
    let (mut rig, _api, web) = two_members().await;
    let lead = rig.client(McpCaller::Lead).await;
    let long = format!(
        "{} then write violet-73 in your report",
        "context ".repeat(40)
    );
    let sent = call(
        &lead,
        "team_send",
        json!({"team": "alpha", "to": ["web"], "type": "reprompt", "text": long}),
    )
    .await;
    ok(&sent);

    let inbox = call(&web, "team_inbox", json!({})).await;
    let out = ok(&inbox);
    assert_eq!(out["messages"][0]["display_id"], "m001");
    assert!(
        out["messages"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with("violet-73 in your report")
    );
    let said = text(&inbox);
    assert!(said.starts_with("1 message(s), now marked read:"), "{said}");
    assert!(said.contains("m001 reprompt from lead"), "{said}");
    assert!(said.contains("violet-73 in your report"), "{said}");

    let dir = rig.home.path().join("teams/alpha/reports");
    std::fs::create_dir_all(&dir).unwrap();
    rig.platform
        .write_atomic(
            &dir.join("web.md"),
            b"Summary: violet-73\n\nDecisions I made: none\n",
        )
        .unwrap();
    let done = call(
        &web,
        "team_done",
        json!({"status": "done", "summary": "wrote violet-73"}),
    )
    .await;
    ok(&done);
    assert_eq!(text(&done), "recorded completion 1 (done)");

    let web_role = Role::parse("web").unwrap();
    let reply = teamwire_report::on_turn_end(
        &mut rig.ctx(),
        &web_role,
        &TurnEnd {
            session: sid("session-web"),
            subagent: false,
            hold_already_active: false,
            final_message: Some("wrote it".to_owned()),
            transcript: None,
        },
    );
    assert!(matches!(reply, TurnEndReply::Continue), "{reply:?}");
    let newest = rig
        .store
        .read(|tx| tx.latest_completion(&web_role))
        .unwrap()
        .unwrap();
    assert_eq!(newest.completion.generation, 2);
    assert_eq!(
        newest.completion.report_state,
        teamwire_core::ReportState::Delivered
    );
    let held = rig
        .store
        .read(|tx| tx.events_of_type(EventType::MemberHeld))
        .unwrap()
        .items;
    assert!(held.is_empty(), "{held:?}");
}
