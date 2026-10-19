//! AFK teams in launch: members join a team that has none yet.

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use std::time::Duration;

use common::{Rig, member_row, pane_member, team_spec};
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

