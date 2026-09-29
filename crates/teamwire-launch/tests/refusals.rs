//! Refusals: a bad spec is reported whole, and nothing is created.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, codes, pane_member, team_spec, worktree_member};
use serde_json::json;
use teamwire_core::SessionId;
use teamwire_fakes::CallKind;

#[test]
fn background_lead_refused_before_anything() {
    let rig = Rig::new();
    rig.harness
        .set_background_caller(SessionId::parse("daemon").unwrap());
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["background_lead"]);
    assert!(rig.home_is_empty(), "a refused launch creates no directory");
    assert_eq!(rig.host.log().count(CallKind::Write), 0);
    assert!(rig.host.log().calls().is_empty(), "zero host calls");
}

#[test]
fn low_disk_refused() {
    let rig = Rig::new();
    rig.platform.inner.set_free_bytes(10 * 1024 * 1024);
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let errors = rig.validate(&spec, false).unwrap_err();
    assert!(codes(&errors).contains(&"low_disk"));
    assert!(rig.home_is_empty());
}

#[test]
fn several_problems_are_reported_together() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let mut no_task = pane_member("c", "Gamma", rig.work.path());
    no_task.as_object_mut().unwrap().remove("task");
    let spec = team_spec(
        "demo",
        &[
            // an implementer in pane mode
            json!({"role": "a", "title": "Alpha", "kind": "implementer", "mode": "pane",
                   "cwd": rig.work.path(), "task": "x"}),
            // a title used twice, ignoring case
            pane_member("b", "alpha", rig.work.path()),
            no_task,
            // the home directory
            pane_member("d", "Delta", &rig.user_home),
            // a repeated role
            worktree_member("e", "Echo", &repo),
            worktree_member("e", "Echo two", &repo),
        ],
    );
    let errors = rig.validate(&spec, false).unwrap_err();
    let found = codes(&errors);
    for code in [
        "implementer_needs_worktree",
        "title_repeated",
        "bad_task",
        "home_directory",
        "duplicate_role",
    ] {
        assert!(found.contains(&code), "{code} missing from {found:?}");
    }
    assert!(rig.home_is_empty());
}

#[test]
fn title_over_28_characters_refused() {
    let rig = Rig::new();
    let long = "x".repeat(29);
    let spec = team_spec("demo", &[pane_member("a", &long, rig.work.path())]);
    assert_eq!(
        codes(&rig.validate(&spec, false).unwrap_err()),
        ["title_too_long"]
    );
}

#[test]
fn host_none_cannot_start_a_member() {
    let mut rig = Rig::new();
    rig.registry
        .add_host(std::sync::Arc::new(teamwire_core::NoHost))
        .unwrap();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let spec = common::with(spec, "host", json!("none"));
    assert_eq!(codes(&rig.validate(&spec, false).unwrap_err()), ["no_host"]);
}

#[test]
fn unknown_harness_refused() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    let spec = common::with(spec, "harness", json!("nonesuch"));
    assert!(codes(&rig.validate(&spec, false).unwrap_err()).contains(&"unknown_harness"));
}

#[test]
fn add_to_a_team_without_a_database_refused() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    assert_eq!(
        codes(&rig.validate(&spec, true).unwrap_err()),
        ["no_such_team"]
    );
    assert!(rig.home_is_empty());
}

#[test]
fn removed_role_never_reused() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    common::seed_team(&rig, "demo", |tx, at| {
        tx.insert_member(at, &common::new_member("a", "Alpha", cwd))?;
        tx.remove_member(
            at,
            &teamwire_core::Role::parse("a").unwrap(),
            None,
            &json!({}),
        )?;
        Ok(())
    });
    let listing = |rig: &Rig| {
        let dir = rig.home.path().join("teams").join("demo");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let before = listing(&rig);
    let spec = team_spec("demo", &[pane_member("a", "Other title", rig.work.path())]);
    let errors = rig.validate(&spec, true).unwrap_err();
    assert_eq!(codes(&errors), ["role_reused"]);
    assert_eq!(rig.host.log().count(CallKind::Write), 0);
    let after = listing(&rig);
    assert_eq!(before, after, "nothing was touched");
}

#[test]
fn failed_before_started_may_be_relaunched_but_started_then_failed_may_not() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    common::seed_team(&rig, "demo", |tx, at| {
        for role in ["a", "b"] {
            tx.insert_member(at, &common::new_member(role, role, cwd))?;
        }
        let (a, b) = (
            teamwire_core::Role::parse("a").unwrap(),
            teamwire_core::Role::parse("b").unwrap(),
        );
        let failed = teamwire_core::MemberState::Failed;
        tx.set_member_state(at, &a, failed, &json!({"reason": "setup"}))?;
        tx.set_member_state(at, &b, teamwire_core::MemberState::Started, &json!({}))?;
        tx.set_member_state(at, &b, failed, &json!({}))?;
        Ok(())
    });
    let spec = team_spec("demo", &[pane_member("a", "Alpha two", cwd)]);
    let plan = rig.validate(&spec, true).unwrap();
    assert!(plan.members.first().unwrap().relaunch);
    let spec = team_spec("demo", &[pane_member("b", "Beta two", cwd)]);
    assert_eq!(
        codes(&rig.validate(&spec, true).unwrap_err()),
        ["role_reused"]
    );
}

#[test]
fn add_to_a_closed_team_refused() {
    let rig = Rig::new();
    common::seed_team(&rig, "demo", |tx, at| {
        tx.begin_close(at, teamwire_core::Pid::new(1))?;
        Ok(())
    });
    let spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    assert_eq!(
        codes(&rig.validate(&spec, true).unwrap_err()),
        ["team_closed"]
    );
}
