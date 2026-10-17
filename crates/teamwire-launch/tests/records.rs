//! What launch records for `reopen`: the model and effort of each started
//! member, and the folder of the lead.

#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, pane_member, role_events, team_spec, with};
use serde_json::json;
use teamwire_core::{Role, SessionId};

#[test]
fn a_started_member_records_its_model_and_effort() {
    let rig = Rig::new();
    let member = with(
        with(
            pane_member("a", "a", rig.work.path()),
            "model",
            json!("opus"),
        ),
        "effort",
        json!("high"),
    );
    let plain = pane_member("b", "b", rig.work.path());
    let spec = team_spec("demo", &[member, plain]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let started = |role: &str| {
        role_events(&store, role)
            .into_iter()
            .find(|(kind, _)| kind == "member.started")
            .unwrap()
            .1
    };
    assert_eq!(started("a")["model"], "opus");
    assert_eq!(started("a")["effort"], "high");
    assert!(started("b")["model"].is_null());
    let options = store
        .read(|tx| tx.run_options(&Role::parse("a").unwrap()))
        .unwrap();
    assert_eq!(options.model.as_deref(), Some("opus"));
    assert_eq!(options.effort.as_deref(), Some("high"));
}

#[test]
fn the_team_records_the_folder_of_the_lead_session() {
    let rig = Rig::new();
    rig.harness
        .set_session_cwd(SessionId::parse("lead-session").unwrap(), "/work/lead");
    let spec = team_spec("demo", &[pane_member("a", "a", rig.work.path())]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let cwd = store.read(|tx| tx.lead_cwd()).unwrap();
    assert_eq!(cwd, Some(std::path::PathBuf::from("/work/lead")));
}
