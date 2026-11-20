//! ADR 0057: the text a model reads from a tool call names what it needs to
//! act on: each member's role, state and generation, and for a refused send
//! that nothing was sent. A model that reads only the text, as a lead did
//! that took a refused send for a sent one, must not need the structured
//! content to tell the two apart.
#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::redundant_closure_for_method_calls,
    reason = "a test indexes the JSON it checks and writes the member's own report as the member would; \
              `ReadTx::members` as a path does not infer the transaction's lifetime"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/rig.rs"]
mod rig;

use rmcp::model::CallToolResult;
use serde_json::json;
use teamwire_core::{EventType, Role, TurnEnd, TurnEndReply};
use teamwire_mcp::McpCaller;

use rig::{call, ok, refused, sid, two_members};

fn text(result: &CallToolResult) -> String {
    result.content[0].as_text().unwrap().text.clone()
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
        .read(|tx| tx.members())
        .unwrap()
        .items
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
    std::fs::write(
        dir.join("web.md"),
        "Summary: violet-73\n\nDecisions I made: none\n",
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
