//! What launch and `--add` hand the host for its labels: the team title as
//! the team row records it, and the member's generation.

#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, member_row, pane_member, team_spec};
use serde_json::json;
use teamwire_core::SpawnRequest;
use teamwire_store::Store;

fn request_of(rig: &Rig, store: &Store, role: &str) -> SpawnRequest {
    let place = member_row(store, role).unwrap().place.unwrap();
    rig.host.spawn_request_of(&place).unwrap()
}

#[test]
fn a_launched_member_gets_the_team_title_and_the_first_generation() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("api", "Api", rig.work.path())]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let req = request_of(&rig, &store, "api");
    assert_eq!((req.team_title.as_str(), req.generation), ("Demo team", 1));
}

#[test]
fn an_added_member_gets_the_recorded_team_title_not_the_add_specs() {
    let rig = Rig::new();
    let first = team_spec("demo", &[pane_member("api", "Api", rig.work.path())]);
    let (_, store) = rig.launch("demo", rig.validate(&first, false).unwrap());
    drop(store);
    let mut more = team_spec("demo", &[pane_member("web", "Web", rig.work.path())]);
    more.as_object_mut()
        .unwrap()
        .insert("title".to_owned(), json!("Another title"));
    let (_, store) = rig.launch("demo", rig.validate(&more, true).unwrap());
    let req = request_of(&rig, &store, "web");
    assert_eq!((req.team_title.as_str(), req.generation), ("Demo team", 1));
}

#[test]
fn a_relaunched_member_shows_the_generation_its_row_holds() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    // A reprompt while the member was still starting raised its generation,
    // then it failed before it started, so `--add` may launch it again.
    common::seed_team(&rig, "demo", |tx, at| {
        let role = teamwire_core::Role::parse("api").unwrap();
        tx.insert_member(at, &common::new_member("api", "Api", cwd))?;
        tx.bump_generation(at, &role, "m001")?;
        tx.set_member_state(at, &role, teamwire_core::MemberState::Failed, &json!({}))?;
        Ok(())
    });
    let spec = team_spec("demo", &[pane_member("api", "Api", cwd)]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, true).unwrap());
    let req = request_of(&rig, &store, "api");
    assert_eq!((req.team_title.as_str(), req.generation), ("Seed", 2));
}
