//! `team_decisions` through a real `rmcp` client: the caller is proven like
//! `team_log`, every kind of member and the lead may read, and the result is
//! the JSON of `teamwire decisions --json`.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    reason = "a test indexes the JSON it checks and writes the reports a member would write"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use std::path::Path;

use serde_json::json;
use teamwire_core::{Clock as _, HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, TeamName};
use teamwire_mcp::McpCaller;
use teamwire_store::{NewMember, NewTeam, Store};

use rig::{Rig, call, ok, refused, token_of, two_members};

fn report(dir: &Path, role: &str, text: &str) {
    let reports = dir.join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(reports.join(format!("{role}.md")), text).unwrap();
}

/// A second team `beta` with one member working in `repo`.
fn add_team(rig: &Rig, name: &str, repo: &std::path::Path) -> Store {
    let team = TeamName::parse(name).unwrap();
    let at = rig.clock.now();
    let mut store = Store::open(rig.home.path(), &team).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at,
                &NewTeam {
                    name: team.clone(),
                    title: name.to_owned(),
                    lead_harness: HarnessId::from_static("fake"),
                    lead_host: HostId::from_static("fake"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            tx.insert_member(
                at,
                &NewMember {
                    role: Role::parse("db").unwrap(),
                    title: "the database".to_owned(),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("fake"),
                    host: HostId::from_static("fake"),
                    cwd: repo.to_path_buf(),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: vec![],
                    deadline_s: 1800,
                },
            )
        })
        .unwrap();
    store
}

#[tokio::test]
async fn a_member_reads_its_own_team_by_default() {
    let (rig, api, _web) = two_members().await;
    let dir = rig.home.path().join("teams/alpha");
    report(&dir, "web", "## Decisions I made\n- used a table\n");
    let result = call(&api, "team_decisions", json!({})).await;
    let out = ok(&result);
    assert_eq!(out["errors"], json!([]));
    assert!(out.get("truncated").is_none());
    let team = &out["teams"][0];
    assert_eq!(team["team"], "alpha");
    let web = team["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "web")
        .unwrap();
    assert_eq!(web["blocks"][0]["text"], "- used a table");
    assert_eq!(web["blocks"][0]["heading"], "## Decisions I made");
}

#[tokio::test]
async fn every_kind_of_member_and_the_lead_may_read() {
    let mut rig = Rig::new();
    for (name, kind) in [
        ("impl", MemberKind::Implementer),
        ("rev", MemberKind::Reviewer),
        ("inv", MemberKind::Investigator),
        ("clo", MemberKind::Closer),
    ] {
        rig.add_member(name, kind);
        let client = rig.member_client(name, &token_of(name)).await;
        let result = call(&client, "team_decisions", json!({})).await;
        assert_eq!(ok(&result)["teams"][0]["team"], "alpha", "{name}");
    }
    let lead = rig.client(McpCaller::Lead).await;
    let result = call(&lead, "team_decisions", json!({"team": "alpha"})).await;
    assert_eq!(ok(&result)["teams"][0]["team"], "alpha");
}

#[tokio::test]
async fn of_reads_another_team_and_grep_reaches_an_archive() {
    let (rig, api, _web) = two_members().await;
    let beta = add_team(&rig, "beta", &rig.home.path().join("work/api"));
    report(
        &rig.home.path().join("teams/beta"),
        "db",
        "Decisions I made:\n- chose WAL mode\n",
    );
    let result = call(&api, "team_decisions", json!({"of": "beta"})).await;
    assert_eq!(
        ok(&result)["teams"][0]["members"][0]["blocks"][0]["text"],
        "- chose WAL mode"
    );

    // A closed team: its folder is moved to the archive, with a checksum file.
    let archived = rig.home.path().join("archive/gamma-2026-09-30");
    std::fs::create_dir_all(&archived).unwrap();
    beta.backup_to(&archived.join("team.db")).unwrap();
    std::fs::write(archived.join("SHA256SUMS"), b"").unwrap();
    report(
        &archived,
        "db",
        "## Decisions I made\n- the archive says wal too\n",
    );
    std::fs::remove_dir_all(rig.home.path().join("teams/beta")).unwrap();
    let result = call(&api, "team_decisions", json!({"grep": "WAL"})).await;
    let out = ok(&result);
    assert_eq!(out["teams"].as_array().unwrap().len(), 1);
    assert_eq!(out["teams"][0]["live"], false);
}

#[tokio::test]
async fn limit_cuts_the_blocks_and_says_so() {
    let (rig, api, _web) = two_members().await;
    let dir = rig.home.path().join("teams/alpha");
    report(
        &dir,
        "api",
        "## Decisions I made\n- a\n## Decisions I made\n- b\n## Decisions I made\n- c\n",
    );
    let result = call(&api, "team_decisions", json!({"limit": 2, "role": "api"})).await;
    let out = ok(&result);
    assert_eq!(out["truncated"], true);
    assert_eq!(
        out["teams"][0]["members"][0]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn an_unknown_team_and_a_wrong_token_are_refused() {
    let (rig, api, _web) = two_members().await;
    let result = call(&api, "team_decisions", json!({"of": "nope"})).await;
    assert_eq!(refused(&result), "refused.unknown_team");
    let intruder = rig.member_client("api", "not-the-token").await;
    let result = call(&intruder, "team_decisions", json!({})).await;
    assert_eq!(refused(&result), "refused.identity");
}

#[tokio::test]
async fn ref_picks_teams_by_provider_and_id() {
    let (rig, api, _web) = two_members().await;
    let mut beta = add_team(&rig, "beta", &rig.home.path().join("work/api"));
    beta.write(|tx| {
        tx.set_refs(&[teamwire_core::TicketRef {
            provider: "linear".to_owned(),
            id: "GKA-580".to_owned(),
        }])
    })
    .unwrap();
    report(
        &rig.home.path().join("teams/beta"),
        "db",
        "## Decisions I made\n- x\n",
    );
    let result = call(&api, "team_decisions", json!({"ref": "linear:gka-580"})).await;
    let out = ok(&result);
    assert_eq!(out["teams"].as_array().unwrap().len(), 1);
    assert_eq!(out["teams"][0]["team"], "beta");
    assert_eq!(
        out["teams"][0]["refs"][0],
        json!({"provider": "linear", "id": "GKA-580"})
    );
    let result = call(&api, "team_decisions", json!({"ref": "jira:GKA-580"})).await;
    assert_eq!(ok(&result)["teams"], json!([]));
}

#[tokio::test]
async fn only_teams_that_share_a_repository_with_the_caller_are_read() {
    let (rig, api, _web) = two_members().await;
    // beta works inside alpha's api directory, in a worktree; gamma elsewhere.
    add_team(&rig, "beta", &rig.home.path().join("work/api/.worktrees/b"));
    add_team(&rig, "gamma", &rig.home.path().join("elsewhere"));
    // A team rooted at the filesystem root shares nothing.
    add_team(&rig, "atroot", std::path::Path::new("/"));
    for team in ["beta", "gamma", "atroot"] {
        report(
            &rig.home.path().join("teams").join(team),
            "db",
            "## Decisions I made\n- x marks\n",
        );
    }
    let result = call(&api, "team_decisions", json!({"grep": "x marks"})).await;
    let names: Vec<_> = ok(&result)["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["team"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["beta"]);
    for team in ["gamma", "atroot"] {
        let result = call(&api, "team_decisions", json!({"of": team})).await;
        assert_eq!(refused(&result), "refused.other_repo", "{team}");
    }
    let result = call(&api, "team_decisions", json!({"of": "beta"})).await;
    assert_eq!(ok(&result)["teams"][0]["team"], "beta");
}
