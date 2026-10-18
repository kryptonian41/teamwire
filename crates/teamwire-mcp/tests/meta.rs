//! The lead's request `_meta` reaches the lead check as `MCP_META_<key>`; a
//! member's server ignores it (ADR 0023 point 8).
#![expect(
    clippy::unwrap_used,
    reason = "an integration test unwraps; a failure there fails the test"
)]

#[path = "../../teamwire-msg/tests/support/hint_harness.rs"]
mod hint_harness;
#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult, MetaObject, RequestMetaObject};
use serde_json::json;
use teamwire_core::{MemberKind, Registry};
use teamwire_mcp::McpCaller;

use hint_harness::HintHarness;
use rig::{Client, Rig, arguments, ok, refused, token_of};

/// A rig whose harness names the caller's session only through
/// `MCP_META_sessionId`, so the process environment proves nothing.
fn meta_rig() -> Rig {
    let mut rig = Rig::new();
    rig.harness.set_caller(None);
    let mut registry = Registry::new();
    registry
        .add_harness(Arc::new(HintHarness::new(
            rig.harness.clone(),
            "MCP_META_sessionId",
        )))
        .unwrap();
    registry.add_host(rig.host.clone()).unwrap();
    rig.registry = Arc::new(registry);
    rig
}

async fn status_with_meta(client: &Client, session: &str) -> CallToolResult {
    let mut params = CallToolRequestParams::new("team_status")
        .with_arguments(arguments(&json!({"team": "alpha"})));
    let meta = arguments(&json!({"sessionId": session}));
    params.meta = Some(RequestMetaObject(MetaObject(meta)));
    client.call_tool(params).await.unwrap()
}

#[tokio::test]
async fn the_lead_is_proven_through_its_meta() {
    let rig = meta_rig();
    let lead = rig.client(McpCaller::Lead).await;
    ok(&status_with_meta(&lead, "lead-1").await);
    let result = status_with_meta(&lead, "someone-else").await;
    assert_eq!(refused(&result), "refused.identity");
}

#[tokio::test]
async fn a_member_server_ignores_meta() {
    let mut rig = meta_rig();
    rig.add_member("api", MemberKind::Implementer);
    // The lead's session in `_meta` does not make a member the lead: a wrong
    // token is still refused.
    let member = rig.member_client("api", "not-the-token").await;
    let result = status_with_meta(&member, "lead-1").await;
    assert_eq!(refused(&result), "refused.identity");
    let member = rig.member_client("api", &token_of("api")).await;
    ok(&status_with_meta(&member, "someone-else").await);
}
