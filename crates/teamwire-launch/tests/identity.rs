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

#[test]
fn identity_written_to_settings_env_mode_0600_outside_worktree() {
    use common::{member_row, one_worktree_team};
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    let rig = Rig::new();
    let (spec, repo) = one_worktree_team(&rig, "demo");
    let plan = rig.validate(&spec, false).unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert_eq!(
        report.members.first().unwrap().outcome,
        teamwire_launch::LaunchOutcome::Started
    );

    let member_dir = rig.home.path().join("teams/demo/members/api");
    let settings = member_dir.join("settings.json");
    let mode = std::fs::metadata(&settings).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let dir_mode = std::fs::metadata(&member_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700);
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    let env = |k: &str| common::jget(&doc, &format!("/env/{k}")).clone();
    assert_eq!(env("TEAMWIRE_TEAM"), "demo");
    assert_eq!(env("TEAMWIRE_ROLE"), "api");
    assert_eq!(env("TEAMWIRE_HOME"), rig.home.path().display().to_string());
    let token = env("TEAMWIRE_TOKEN").as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64, "32 random bytes, hex");

    // The store holds only the hash.
    let role = teamwire_core::Role::parse("api").unwrap();
    let stored = store
        .read(|tx| tx.member_token_hash(&role))
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.as_slice(),
        Sha256::digest(token.as_bytes()).as_slice()
    );

    // The host's own spawn environment carries nothing: the channel is the file.
    let place = member_row(&store, "api").unwrap().place.unwrap();
    let (_, argv, spawn_env) = rig.host.spawned_with(&place).unwrap();
    assert!(spawn_env.is_empty());
    assert!(argv.iter().all(|a| !a.contains(&token)));

    // The token is in no file of the worktree, and in no event.
    let mut files = Vec::new();
    common::all_files_under(&repo.join(".worktrees/api"), &mut files);
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            assert!(!text.contains(&token), "token in {}", file.display());
        }
    }
    let events = store
        .read(|tx| tx.events_of_role(&role)?.all_or_corrupt())
        .unwrap();
    assert!(!format!("{events:?}").contains(&token));
}

#[test]
fn host_environment_channel_puts_identity_in_the_spawn_request_only() {
    let rig = Rig::new();
    let mut harness_caps = FakeHarness::caps();
    harness_caps.session_env_via_config = false;
    rig.harness.set_caps(harness_caps);
    let mut host_caps = FakeHost::caps();
    host_caps.sets_env_at_spawn = true;
    rig.host.set_caps(host_caps);
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    let (_, store) = rig.launch("demo", plan);
    let place = common::member_row(&store, "a").unwrap().place.unwrap();
    let (_, _, env) = rig.host.spawned_with(&place).unwrap();
    let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        names,
        [
            "TEAMWIRE_TEAM",
            "TEAMWIRE_ROLE",
            "TEAMWIRE_HOME",
            "TEAMWIRE_TOKEN"
        ]
    );
}
