//! Launch step 7 records the host's session for a member only when the
//! member's row has none: a session-start hook that ran first wins
//! (ADR 0023 point 5).
#![expect(
    clippy::unwrap_used,
    reason = "an integration test unwraps; a failure there fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;
#[path = "../../teamwire-msg/tests/support/hint_harness.rs"]
mod hint_harness;

use std::sync::Arc;

use common::{Rig, member_row, pane_member, team_spec};
use hint_harness::HintHarness;
use teamwire_core::{Occupant, Registry, Role, SessionId, TeamName};
use teamwire_fakes::FakeHarness;
use teamwire_store::Store;

fn session(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

/// A rig whose harness does not choose its session id. The session starts
/// by recording `by_hook` (when given) as the hook would, and the host then
/// reports `occupant-session` as the pane's occupant.
fn rig_with(by_hook: Option<&'static str>) -> Rig {
    let mut rig = Rig::new();
    let mut caps = FakeHarness::caps();
    caps.choose_session_id = false;
    rig.harness.set_caps(caps);
    let host = Arc::clone(&rig.host);
    let home = rig.home.path().to_path_buf();
    let harness = HintHarness::new(Arc::clone(&rig.harness), "unused").on_startup(move |place| {
        host.set_occupant(
            place,
            Occupant {
                session: Some(session("occupant-session")),
                pids: vec![],
            },
        );
        if let Some(id) = by_hook {
            let mut store = Store::open_existing(&home, &TeamName::parse("demo").unwrap()).unwrap();
            let role = Role::parse("a").unwrap();
            store
                .write(|tx| tx.set_member_session(&role, &session(id)))
                .unwrap();
        }
    });
    let mut registry = Registry::new();
    registry.add_harness(Arc::new(harness)).unwrap();
    registry.add_host(rig.host.clone()).unwrap();
    rig.registry = registry;
    rig
}

fn launched_session(rig: &Rig) -> Option<SessionId> {
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    let (_, store) = rig.launch("demo", plan);
    member_row(&store, "a").and_then(|m| m.session_id)
}

#[test]
fn the_hosts_session_is_recorded_when_none_is() {
    assert_eq!(
        launched_session(&rig_with(None)),
        Some(session("occupant-session"))
    );
}

#[test]
fn a_session_the_hook_recorded_is_kept() {
    assert_eq!(
        launched_session(&rig_with(Some("hook-session"))),
        Some(session("hook-session"))
    );
}
