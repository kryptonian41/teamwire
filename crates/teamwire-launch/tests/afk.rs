//! AFK teams in launch: members join a team that has none yet, a spent
//! budget turns away new implementers only, and a member below the AFK
//! model table gets a warning.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "an integration test unwraps and indexes; a failure there fails the test"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use std::time::Duration;

use common::{Rig, member_row, pane_member, team_spec, worktree_member};
use serde_json::json;
use teamwire_core::{Clock, HarnessId, HostId, LeadRoute, SessionId, SessionKind, TeamName};
use teamwire_fakes::FakeHarness;
use teamwire_launch::LaunchOutcome;
use teamwire_store::{NewTeam, Store};

/// An AFK team with no members whose budget ends `budget` after the rig's now.
fn afk_team(rig: &Rig, team: &str, budget: Duration) {
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(rig.home.path(), &name).unwrap();
    let at = rig.clock.now();
    store
        .write(|tx| {
            tx.create_afk_team(
                at,
                &NewTeam {
                    name: name.clone(),
                    title: "Afk run".to_owned(),
                    lead_harness: HarnessId::from_static(FakeHarness::ID),
                    lead_host: HostId::from_static(teamwire_fakes::FakeHost::ID),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((
                        SessionId::parse("lead-session").unwrap(),
                        SessionKind::Interactive,
                    )),
                },
                at.checked_add(budget).unwrap(),
                &json!({}),
            )
            .map(|_| ())
        })
        .unwrap();
}

fn add(rig: &Rig, children: &[serde_json::Value]) -> (teamwire_launch::LaunchReport, Store) {
    let plan = rig.validate(&team_spec("run", children), true).unwrap();
    rig.launch("run", plan)
}

#[test]
fn members_join_an_afk_team_that_has_none() {
    let rig = Rig::new();
    afk_team(&rig, "run", Duration::from_secs(3600));
    let (report, store) = add(&rig, &[pane_member("scout", "Scout", rig.work.path())]);
    assert_eq!(report.members[0].outcome, LaunchOutcome::Started);
    assert!(member_row(&store, "scout").is_some());
}

#[test]
fn past_the_budget_an_implementer_is_refused_and_every_other_kind_starts() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    afk_team(&rig, "run", Duration::from_secs(3600));
    rig.clock.advance(Duration::from_secs(3601));
    let reviewer = json!({"role": "rev", "title": "Rev", "kind": "reviewer", "mode": "pane",
                          "cwd": rig.work.path(), "task": "review"});
    let closer = json!({"role": "clo", "title": "Clo", "kind": "closer", "mode": "pane",
                        "cwd": rig.work.path(), "task": "close"});
    let (report, store) = add(
        &rig,
        &[
            worktree_member("api", "API", &repo),
            pane_member("scout", "Scout", rig.work.path()),
            reviewer,
            closer,
        ],
    );
    let outcomes: Vec<_> = report.members.iter().map(|m| &m.outcome).collect();
    let LaunchOutcome::Refused(why) = outcomes[0] else {
        panic!("the implementer was not refused: {outcomes:?}");
    };
    assert!(why.starts_with("afk_budget_spent"), "{why}");
    assert!(outcomes[1..].iter().all(|o| **o == LaunchOutcome::Started));
    assert!(
        member_row(&store, "api").is_none(),
        "a refused member leaves no row"
    );
}

#[test]
fn before_the_budget_ends_an_implementer_starts() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    afk_team(&rig, "run", Duration::from_secs(3600));
    rig.clock.advance(Duration::from_secs(3599));
    let (report, _) = add(&rig, &[worktree_member("api", "API", &repo)]);
    assert_eq!(report.members[0].outcome, LaunchOutcome::Started);
}

#[test]
fn an_ordinary_team_has_no_budget() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    common::seed_team(&rig, "run", |_, _| Ok(()));
    rig.clock.advance(Duration::from_secs(1_000_000));
    let (report, _) = add(&rig, &[worktree_member("api", "API", &repo)]);
    assert_eq!(report.members[0].outcome, LaunchOutcome::Started);
}

fn model_member(
    role: &str,
    kind: &str,
    model: Option<&str>,
    effort: Option<&str>,
    cwd: &std::path::Path,
) -> serde_json::Value {
    let mut m = json!({"role": role, "title": role, "kind": kind, "mode": "pane",
                       "cwd": cwd, "task": "x"});
    if let Some(model) = model {
        m["model"] = json!(model);
    }
    if let Some(effort) = effort {
        m["effort"] = json!(effort);
    }
    m
}

fn warnings_for(rig: &Rig, team_afk: bool, children: &[serde_json::Value]) -> Vec<String> {
    if team_afk {
        afk_team(rig, "run", Duration::from_secs(3600));
    } else {
        common::seed_team(rig, "run", |_, _| Ok(()));
    }
    rig.validate(&team_spec("run", children), true)
        .unwrap()
        .warnings
}

#[test]
fn a_member_below_the_afk_table_gets_one_warning_each() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let warnings = warnings_for(
        &rig,
        true,
        &[
            model_member("a", "investigator", Some("sonnet"), Some("high"), cwd),
            model_member("b", "reviewer", Some("opus"), Some("high"), cwd),
            model_member("c", "closer", None, None, cwd),
            model_member("d", "investigator", Some("opus"), Some("max"), cwd),
            model_member("e", "reviewer", Some("opus"), Some("xhigh"), cwd),
            model_member("f", "investigator", Some("other-model"), Some("high"), cwd),
        ],
    );
    let afk: Vec<_> = warnings.iter().filter(|w| w.contains("AFK")).collect();
    assert_eq!(afk.len(), 3, "{warnings:?}");
    for (w, role) in afk.iter().zip(["a", "b", "c"]) {
        assert!(w.contains(&format!("member {role} ")), "{w}");
    }
}

#[test]
fn an_ordinary_team_gets_no_afk_warning() {
    let rig = Rig::new();
    let warnings = warnings_for(
        &rig,
        false,
        &[model_member(
            "a",
            "investigator",
            Some("haiku"),
            Some("low"),
            rig.work.path(),
        )],
    );
    assert!(warnings.iter().all(|w| !w.contains("AFK")), "{warnings:?}");
}
