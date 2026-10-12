//! Other teams through the MCP tools: `team_teams` lists them,
//! `team_members` reads one with `of`, and `team_send` reaches its agents
//! as `role@team` (MSG-20, MSG-21).
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "an integration test unwraps; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use serde_json::json;
use teamwire_core::{
    Clock as _, HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode, Role, TeamName,
};
use teamwire_mcp::McpCaller;
use teamwire_store::{NewMember, NewTeam, Store};

use rig::{Rig, call, ok, refused, sid, two_members};

/// Team `beta` beside the rig's `alpha`, led from `lead-2`, with an
/// implementer `db` whose session can be typed to.
fn add_beta(rig: &mut Rig) {
    let team = TeamName::parse("beta").unwrap();
    let at = rig.clock.now();
    let mut store = Store::open(rig.home.path(), &team).unwrap();
    let db = Role::parse("db").unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at,
                &NewTeam {
                    name: team.clone(),
                    title: "Beta".to_owned(),
                    lead_harness: HarnessId::from_static("fake"),
                    lead_host: HostId::from_static("fake"),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((sid("lead-2"), teamwire_core::SessionKind::Interactive)),
                },
            )?;
            tx.insert_member(
                at,
                &NewMember {
                    role: db.clone(),
                    title: "the database".to_owned(),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("fake"),
                    host: HostId::from_static("fake"),
                    cwd: rig.home.path().join("work/beta/db"),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: vec![],
                    deadline_s: 1800,
                },
            )?;
            tx.set_member_session(&db, &sid("session-beta-db"))?;
            tx.set_member_state(at, &db, MemberState::Started, &json!({}))
        })
        .unwrap();
    rig.healthy("lead-2");
    rig.healthy("session-beta-db");
}

#[tokio::test]
async fn a_member_lists_every_team_and_reads_another_teams_roster() {
    let (mut rig, api, _web) = two_members().await;
    add_beta(&mut rig);
    let out = ok(&call(&api, "team_teams", json!({})).await).clone();
    let names: Vec<&str> = out["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
    assert_eq!(out["teams"][1]["state"], "open");
    assert_eq!(out["teams"][1]["title"], "Beta");
    assert_eq!(out["teams"][1]["members"][0]["role"], "db");

    let out = call(&api, "team_members", json!({"of": "beta"})).await;
    let members = &ok(&out)["members"];
    assert_eq!(members.as_array().unwrap().len(), 1);
    assert_eq!(members[0]["role"], "db");
    assert_eq!(
        members[0]["accepts_from_members"],
        json!(["notice", "finding", "question", "answer"])
    );

    let out = call(&api, "team_members", json!({"of": "gamma"})).await;
    assert_eq!(refused(&out), "refused.unknown_team");
}

#[tokio::test]
async fn a_member_sends_to_an_agent_of_another_team() {
    let (mut rig, api, _web) = two_members().await;
    add_beta(&mut rig);
    let out = call(
        &api,
        "team_send",
        json!({"to": ["db@beta"], "type": "finding", "text": "the cache key changed"}),
    )
    .await;
    let out = ok(&out);
    assert_eq!(out["deliveries"][0]["to"], "db@beta");
    assert_eq!(out["deliveries"][0]["state"], "handed_off");

    let out = call(
        &api,
        "team_send",
        json!({"to": ["db@gamma"], "type": "finding", "text": "x"}),
    )
    .await;
    assert_eq!(refused(&out), "refused.unknown_team");
}

#[tokio::test]
async fn the_lead_may_not_instruct_another_teams_member() {
    let (mut rig, _api, _web) = two_members().await;
    add_beta(&mut rig);
    let lead = rig.client(McpCaller::Lead).await;
    let out = call(
        &lead,
        "team_send",
        json!({"team": "alpha", "to": ["db@beta"], "type": "note", "text": "do this"}),
    )
    .await;
    assert_eq!(refused(&out), "refused.recipient_not_allowed");
}
