//! How a member gets its identity, and when launch refuses for lack of a channel.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, codes, pane_member, team_spec};
use teamwire_core::{HostCaps, IdentityChannel};
use teamwire_fakes::{FakeHarness, FakeHost};

#[test]
fn no_identity_channel_refused() {
    let rig = Rig::new();
    let mut harness_caps = FakeHarness::caps();
    harness_caps.session_env_via_config = false;
    rig.harness.set_caps(harness_caps);
    let host_caps: HostCaps = FakeHost::caps();
    assert!(!host_caps.sets_env_at_spawn, "the fake host is herdr-like");
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["no_identity_channel"]);
    assert_eq!(errors.first().and_then(|e| e.role.as_deref()), Some("a"));
    assert!(rig.home_is_empty());
    assert!(rig.host.log().calls().iter().all(|c| c.name != "spawn"));
}

#[test]
fn harness_config_is_chosen_first_then_host_environment() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    assert_eq!(
        plan.members.first().unwrap().channel,
        IdentityChannel::HarnessConfig
    );

    let mut harness_caps = FakeHarness::caps();
    harness_caps.session_env_via_config = false;
    rig.harness.set_caps(harness_caps);
    let mut host_caps = FakeHost::caps();
    host_caps.sets_env_at_spawn = true;
    rig.host.set_caps(host_caps);
    let plan = rig.validate(&spec, false).unwrap();
    assert_eq!(
        plan.members.first().unwrap().channel,
        IdentityChannel::HostEnv
    );
}
