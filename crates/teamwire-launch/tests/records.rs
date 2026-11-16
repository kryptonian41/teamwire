//! What launch records for `reopen`: the model and effort of each started
//! member, and the folder of the lead. And the folders launch makes before a
//! member starts: `reports/` in the team directory, and each member's scratch
//! folder outside it.

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

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Rig, launch_one, pane_member, role_events, team_spec, with};
use serde_json::json;
use teamwire_core::{
    Address, Harness, Host, HostCaps, HostError, Key, Occupant, OneLine, Place, PlaceState,
    Registry, Role, SessionId, SpawnRequest,
};
use teamwire_fakes::FakeHost;

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

#[test]
fn a_started_member_records_the_merged_handoff_threshold() {
    let rig = Rig::new();
    let own = with(
        pane_member("a", "a", rig.work.path()),
        "context_handoff_tokens",
        json!(150_000),
    );
    let inherits = pane_member("b", "b", rig.work.path());
    let spec = with(
        team_spec("demo", &[own, inherits]),
        "context_handoff_tokens",
        json!(300_000),
    );
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let recorded = |role: &str| {
        role_events(&store, role)
            .into_iter()
            .find(|(kind, _)| kind == "member.started")
            .unwrap()
            .1["context_handoff_tokens"]
            .clone()
    };
    assert_eq!(recorded("a"), json!(150_000));
    assert_eq!(recorded("b"), json!(300_000));
}

#[test]
fn an_unset_handoff_threshold_is_recorded_as_null() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "a", rig.work.path())]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let started = role_events(&store, "a")
        .into_iter()
        .find(|(kind, _)| kind == "member.started")
        .unwrap()
        .1;
    assert!(
        started
            .as_object()
            .unwrap()
            .contains_key("context_handoff_tokens")
    );
    assert!(started["context_handoff_tokens"].is_null());
}

#[test]
fn member_creating_records_the_task_class_or_null() {
    let rig = Rig::new();
    let classed = with(
        pane_member("a", "a", rig.work.path()),
        "task_class",
        json!("root_cause"),
    );
    let plain = pane_member("b", "b", rig.work.path());
    let spec = team_spec("demo", &[classed, plain]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let creating = |role: &str| {
        role_events(&store, role)
            .into_iter()
            .find(|(kind, _)| kind == "member.creating")
            .unwrap()
            .1
    };
    assert_eq!(creating("a")["task_class"], "root_cause");
    let unset = creating("b");
    assert!(unset.as_object().unwrap().contains_key("task_class"));
    assert!(unset["task_class"].is_null());
}

/// The fake host, which notes whether a folder exists when the first member
/// is spawned. A member may write its report as soon as it starts, so a
/// check made after launch returns cannot show the order (LCH-31).
struct SpawnWatch {
    inner: Arc<FakeHost>,
    folder: PathBuf,
    at_first_spawn: Mutex<Option<bool>>,
}

impl Host for SpawnWatch {
    fn id(&self) -> teamwire_core::HostId {
        self.inner.id()
    }
    fn capabilities(&self) -> HostCaps {
        self.inner.capabilities()
    }
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
        // A poisoned lock leaves `None`, which fails the test's assert.
        if let Ok(mut seen) = self.at_first_spawn.lock() {
            seen.get_or_insert_with(|| self.folder.is_dir());
        }
        self.inner.spawn(req, timeout)
    }
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError> {
        self.inner.submit(place, text, timeout)
    }
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        self.inner.state(place, timeout)
    }
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
        self.inner.occupant(place, timeout)
    }
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        self.inner.read_tail(place, lines, timeout)
    }
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        self.inner.send_keys(place, keys, timeout)
    }
    fn close(&self, place: &Place, expected: &Address, timeout: Duration) -> Result<(), HostError> {
        self.inner.close(place, expected, timeout)
    }
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        self.inner.list(timeout)
    }
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        self.inner.live_agent_names(timeout)
    }
    fn place_env_var(&self) -> Option<&'static str> {
        self.inner.place_env_var()
    }
}

#[test]
fn launch_makes_the_reports_folder_before_the_first_member_writes() {
    let mut rig = Rig::new();
    let watch = Arc::new(SpawnWatch {
        inner: Arc::clone(&rig.host),
        folder: rig.home.path().join("teams/demo/reports"),
        at_first_spawn: Mutex::new(None),
    });
    let mut registry = Registry::new();
    registry
        .add_harness(Arc::clone(&rig.harness) as Arc<dyn Harness>)
        .unwrap();
    registry
        .add_host(Arc::clone(&watch) as Arc<dyn Host>)
        .unwrap();
    rig.registry = registry;
    let (report, _store) = launch_one(&rig, "demo", "a", false);
    assert_eq!(
        *watch.at_first_spawn.lock().unwrap(),
        Some(true),
        "the reports folder must exist when the first member starts"
    );
    assert!(
        matches!(
            report.members.first().map(|m| &m.outcome),
            Some(teamwire_launch::LaunchOutcome::Started)
        ),
        "{report:?}"
    );
    assert!(rig.home.path().join("teams/demo/reports").is_dir());
}

#[test]
fn launch_add_makes_the_reports_folder_when_it_is_missing() {
    let rig = Rig::new();
    drop(launch_one(&rig, "demo", "a", false));
    let reports = rig.home.path().join("teams/demo/reports");
    std::fs::remove_dir(&reports).unwrap();
    drop(launch_one(&rig, "demo", "b", true));
    assert!(reports.is_dir());
}

#[test]
fn a_started_member_has_its_own_scratch_folder_outside_the_team_directory() {
    let rig = Rig::new();
    drop(launch_one(&rig, "demo", "a", false));
    drop(launch_one(&rig, "demo", "b", true));
    for role in ["a", "b"] {
        assert!(
            rig.home.path().join("scratch/demo").join(role).is_dir(),
            "{role}"
        );
    }
}

#[test]
fn a_member_whose_start_fails_leaves_no_scratch_folder() {
    use teamwire_core::{HostError, HostErrorCode};
    use teamwire_fakes::Fault;
    let rig = Rig::new();
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::BadArgument, "no")),
        1,
    );
    let (report, _store) = launch_one(&rig, "demo", "a", false);
    let outcome = report.members.first().map(|m| &m.outcome);
    assert!(
        matches!(outcome, Some(teamwire_launch::LaunchOutcome::Failed(r)) if r.contains("scratch folder removed")),
        "{report:?}"
    );
    assert!(!rig.home.path().join("scratch/demo/a").exists());
}
