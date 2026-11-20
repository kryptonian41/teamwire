//! The reconciler kind, the `quiet` setting and the set label at launch:
//! what is refused, what is stored, what the events and the seat file say,
//! and what a relaunch after a failed start keeps.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, codes, jget, pane_member, role_events, team_spec, worktree_member};
use serde_json::{Value, json};
use teamwire_core::{MemberKind, MemberState, Role};
use teamwire_launch::{LaunchOutcome, unknown_key_warnings};

fn reconciler(role: &str, title: &str, cwd: &std::path::Path) -> Value {
    common::with(pane_member(role, title, cwd), "kind", json!("reconciler"))
}

#[test]
fn a_reconciler_in_pane_mode_validates() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[reconciler("rec", "Reconcile", rig.work.path())]);
    let plan = rig.validate(&spec, false).unwrap();
    assert_eq!(
        plan.members.first().map(|m| m.kind),
        Some(MemberKind::Reconciler)
    );
}

#[test]
fn a_reconciler_in_worktree_mode_is_refused() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    let rec = common::with(
        worktree_member("rec", "Reconcile", &repo),
        "kind",
        json!("reconciler"),
    );
    let errors = rig.validate(&team_spec("demo", &[rec]), false).unwrap_err();
    assert_eq!(codes(&errors), ["reconciler_needs_pane"]);
    let refusal = errors.first().unwrap();
    assert_eq!(refusal.field.as_deref(), Some("mode"));
    assert!(
        refusal
            .message
            .contains("role rec is a reconciler, so it needs mode pane"),
        "{}",
        refusal.message
    );
    assert!(rig.home_is_empty());
}

/// `"quiet": false` on a kind that is quiet by nature is refused. No key,
/// and `true`, are both accepted, and so is `false` on a kind that talks.
#[test]
fn quiet_false_on_a_quiet_kind_is_refused() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    for kind in ["reviewer", "closer", "plan_reviewer", "reconciler"] {
        let member = common::with(
            common::with(pane_member("q", "Quiet one", cwd), "kind", json!(kind)),
            "quiet",
            json!(false),
        );
        let errors = rig
            .validate(&team_spec("demo", &[member]), false)
            .unwrap_err();
        assert_eq!(codes(&errors), ["quiet_cannot_be_unset"], "{kind}");
        assert_eq!(errors.first().unwrap().field.as_deref(), Some("quiet"));
        for accepted in [None, Some(true)] {
            let mut member = common::with(pane_member("q", "Quiet one", cwd), "kind", json!(kind));
            if let Some(quiet) = accepted {
                member = common::with(member, "quiet", json!(quiet));
            }
            assert!(
                rig.validate(&team_spec("demo", &[member]), false).is_ok(),
                "{kind} {accepted:?}"
            );
        }
    }
    let talker = common::with(pane_member("t", "Talker", cwd), "quiet", json!(false));
    assert!(rig.validate(&team_spec("demo", &[talker]), false).is_ok());
    assert!(rig.home_is_empty());
}

#[test]
fn a_set_label_follows_the_role_name_rule() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let with_set = |set: &str| common::with(pane_member("r", "Reader", cwd), "set", json!(set));
    for good in ["rev1", "a", "plan_set_2", "abcdefghijklmnop"] {
        assert!(
            rig.validate(&team_spec("demo", &[with_set(good)]), false)
                .is_ok(),
            "{good}"
        );
    }
    for bad in [
        "",
        "Rev1",
        "1rev",
        "rev-1",
        "_rev",
        "rev 1",
        "abcdefghijklmnopq",
        "révue",
    ] {
        let errors = rig
            .validate(&team_spec("demo", &[with_set(bad)]), false)
            .unwrap_err();
        assert_eq!(codes(&errors), ["set_label_invalid"], "{bad:?}");
        assert_eq!(errors.first().unwrap().field.as_deref(), Some("set"));
    }
    assert!(rig.home_is_empty());
}

#[test]
fn quiet_and_set_are_known_keys() {
    let text = r#"{"team":"demo","title":"T","children":[
        {"role":"api","title":"A","mode":"pane","task":"t","quiet":true,"set":"rev1"}]}"#;
    assert!(unknown_key_warnings(text).is_empty());
}

/// Launch stores what the spec said, and writes `quiet` and `set` into the
/// member's events only when the spec sets them.
#[test]
fn launch_stores_the_marks_and_names_them_in_the_events() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let hush = common::with(
        common::with(pane_member("hush", "Hush", cwd), "quiet", json!(true)),
        "set",
        json!("rev1"),
    );
    let rec = common::with(reconciler("rec", "Reconcile", cwd), "set", json!("rev1"));
    let plain = pane_member("plain", "Plain", cwd);
    let plan = rig
        .validate(&team_spec("demo", &[hush, rec, plain]), false)
        .unwrap();
    let (report, store) = rig.launch("demo", plan);
    assert!(
        report
            .members
            .iter()
            .all(|m| m.outcome == LaunchOutcome::Started),
        "{report:?}"
    );
    for (role, quiet, set) in [
        ("hush", true, Some("rev1")),
        ("rec", false, Some("rev1")),
        ("plain", false, None),
    ] {
        let row = common::member_row(&store, role).unwrap();
        assert_eq!((row.quiet, row.set.as_deref()), (quiet, set), "{role}");
        assert_eq!(row.is_quiet(), role != "plain", "{role}");
    }
    let creating = |role: &str| {
        role_events(&store, role)
            .into_iter()
            .find(|(kind, _)| kind == "member.creating")
            .map(|(_, data)| data)
            .unwrap()
    };
    let hush = creating("hush");
    assert_eq!(jget(&hush, "/quiet"), &json!(true));
    assert_eq!(jget(&hush, "/set"), &json!("rev1"));
    let rec = creating("rec");
    assert!(rec.get("quiet").is_none(), "{rec}");
    assert_eq!(jget(&rec, "/set"), &json!("rev1"));
    let plain = creating("plain");
    assert!(plain.get("quiet").is_none(), "{plain}");
    assert!(plain.get("set").is_none(), "{plain}");
    assert!(plain.get("task_class").is_some(), "{plain}");
}

/// A role that failed before it started is launched again over its old
/// row. The new spec's marks replace the old ones, including no marks.
#[test]
fn a_relaunched_member_takes_the_new_specs_marks() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    common::seed_team(&rig, "demo", |tx, at| {
        for role in ["was", "now"] {
            tx.insert_member(at, &common::new_member(role, role, cwd))?;
        }
        let (was, now) = (Role::parse("was").unwrap(), Role::parse("now").unwrap());
        tx.set_member_marks(&was, true, Some("old"))?;
        let failed = MemberState::Failed;
        tx.set_member_state(at, &was, failed, &json!({"reason": "setup"}))?;
        tx.set_member_state(at, &now, failed, &json!({"reason": "setup"}))?;
        Ok(())
    });
    let was = common::with(pane_member("was", "Was quiet", cwd), "set", json!("new"));
    let now = common::with(pane_member("now", "Now quiet", cwd), "quiet", json!(true));
    let plan = rig.validate(&team_spec("demo", &[was, now]), true).unwrap();
    assert!(plan.members.iter().all(|m| m.relaunch));
    let (report, store) = rig.launch("demo", plan);
    assert!(
        report
            .members
            .iter()
            .all(|m| m.outcome == LaunchOutcome::Started),
        "{report:?}"
    );
    let was = common::member_row(&store, "was").unwrap();
    assert_eq!((was.quiet, was.set.as_deref()), (false, Some("new")));
    assert!(!was.is_quiet());
    let now = common::member_row(&store, "now").unwrap();
    assert_eq!((now.quiet, now.set.as_deref()), (true, None));
    assert!(now.is_quiet());
}

/// The seat file of an investigator whose spec set `quiet` holds the quiet
/// talking part and asks the parent in the report file, as a reviewer's
/// does. An investigator without the setting keeps the talking parts.
#[test]
fn a_quiet_investigators_seat_file_holds_the_quiet_parts() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let hush = common::with(pane_member("hush", "Hush", cwd), "quiet", json!(true));
    let plain = pane_member("plain", "Plain", cwd);
    let plan = rig
        .validate(&team_spec("demo", &[hush, plain]), false)
        .unwrap();
    let (report, _store) = rig.launch("demo", plan);
    assert!(
        report
            .members
            .iter()
            .all(|m| m.outcome == LaunchOutcome::Started),
        "{report:?}"
    );
    let seat = |role: &str| {
        std::fs::read_to_string(
            rig.home
                .path()
                .join("teams/demo/contract")
                .join(format!("{role}.md")),
        )
        .unwrap()
    };
    let (hush, plain) = (seat("hush"), seat("plain"));
    assert!(hush.contains("You are the INVESTIGATOR"), "{hush}");
    assert!(
        hush.contains("You send no messages, and members do not message you"),
        "{hush}"
    );
    assert!(
        hush.contains("question in your report file under a heading 'Questions for the"),
        "{hush}"
    );
    assert!(!hush.contains(" send --to "), "{hush}");
    assert!(
        plain.contains("question message (TALKING TO THE TEAM above)"),
        "{plain}"
    );
    assert!(plain.contains(" send --to "), "{plain}");
}
