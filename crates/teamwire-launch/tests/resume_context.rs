//! A resumed member gets the binary and the plugin switches its first start
//! used: a harness that carries hook and MCP commands in argv (Codex) would
//! otherwise run a bare `teamwire` and lose the switches.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;
#[expect(dead_code, reason = "this file uses part of the shared harness")]
#[path = "../../teamwire-msg/tests/support/hint_harness.rs"]
mod hint_harness;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use common::{Rig, member_row, pane_member, team_spec, with};
use hint_harness::{HintHarness, Resumes};
use serde_json::json;
use teamwire_core::{Clock, Host, Registry, Role, SessionId};
use teamwire_launch::{ReopenOpts, reopen};
use teamwire_store::Ctx;

fn reopen_with(plugins_in_spec: bool, bin: Option<&str>) -> Vec<teamwire_core::MemberLaunch> {
    let mut rig = Rig::new();
    let resumes: Resumes = Arc::new(Mutex::new(Vec::new()));
    let harness = HintHarness::new(Arc::clone(&rig.harness), "unused")
        .recording_resumes(Arc::clone(&resumes));
    let mut registry = Registry::new();
    registry.add_harness(Arc::new(harness)).unwrap();
    registry.add_host(rig.host.clone()).unwrap();
    rig.registry = registry;
    rig.harness
        .set_session_cwd(SessionId::parse("lead-session").unwrap(), rig.work.path());
    let mut spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    if plugins_in_spec {
        spec = with(spec, "plugins", json!({"tools@market": true}));
    }
    let (_, mut store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let place = member_row(&store, "a").unwrap().place.unwrap();
    rig.host
        .close(&place, std::time::Duration::from_secs(5))
        .unwrap();
    rig.harness
        .set_interactive_caller(SessionId::parse("other").unwrap());
    let opts = ReopenOpts {
        role: Some(Role::parse("a").unwrap()),
        teamwire_bin: bin.map(PathBuf::from),
        ..ReopenOpts::default()
    };
    let mut ctx = Ctx {
        home: rig.home.path(),
        store: &mut store,
        registry: &rig.registry,
        platform: &*rig.platform,
        clock: &rig.clock as &dyn Clock,
        config: &rig.config,
    };
    reopen(&mut ctx, &opts).unwrap();
    resumes.lock().unwrap().clone()
}

#[test]
fn a_resume_names_the_running_binary_and_the_recorded_plugins() {
    let launches = reopen_with(true, Some("/opt/tw/teamwire"));
    let launch = launches.first().unwrap();
    assert_eq!(launch.teamwire_bin, PathBuf::from("/opt/tw/teamwire"));
    assert_eq!(launch.plugins.get("tools@market"), Some(&true));
}

#[test]
fn a_member_started_without_recorded_plugins_resumes_with_none() {
    let launches = reopen_with(false, None);
    let launch = launches.first().unwrap();
    assert_eq!(launch.teamwire_bin, PathBuf::from("teamwire"));
    assert!(launch.plugins.is_empty());
}
