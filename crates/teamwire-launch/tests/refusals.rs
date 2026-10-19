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
use teamwire_core::{Clock, SessionId};
use teamwire_fakes::CallKind;
use teamwire_launch::parse_spec;

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

#[test]
fn hanging_agent_start_fails_one_member_only() {
    use teamwire_core::MemberState;
    use teamwire_fakes::Fault;
    use teamwire_launch::LaunchOutcome;
    let rig = Rig::new();
    rig.host.inject("spawn", Fault::Hang, 1);
    let repo = rig.repo("proj");
    let spec = team_spec(
        "demo",
        &[
            worktree_member("first", "First", &repo),
            worktree_member("second", "Second", &repo),
        ],
    );
    let plan = rig.validate(&spec, false).unwrap();
    let (report, store) = rig.launch("demo", plan);
    let outcomes: Vec<&LaunchOutcome> = report.members.iter().map(|m| &m.outcome).collect();
    let first = format!("{:?}", outcomes.first().unwrap());
    assert!(
        first.starts_with("Failed(") && first.contains("timeout"),
        "{first}"
    );
    assert_eq!(outcomes.get(1).copied(), Some(&LaunchOutcome::Started));
    assert_eq!(
        common::member_row(&store, "first").unwrap().state,
        MemberState::Failed
    );
    assert_eq!(
        common::member_row(&store, "second").unwrap().state,
        MemberState::Started
    );
    assert!(
        !repo.join(".worktrees/first").exists(),
        "the failed member's checkout is taken back"
    );
    assert!(repo.join(".worktrees/second").is_dir());
}

#[test]
fn a_not_ready_host_is_asked_once_and_a_taken_name_refuses_the_member() {
    use teamwire_core::{HostError, HostErrorCode};
    use teamwire_fakes::Fault;
    use teamwire_launch::LaunchOutcome;
    // The host adapter owns any retry: a second spawn would start the same
    // name again in a new place.
    let rig = Rig::new();
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::NotReady, "agent_not_ready")),
        1,
    );
    let (report, _store) = rig.launch("demo", rig.validate(&spec_for(&rig), false).unwrap());
    assert!(matches!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Failed(_)
    ));
    assert_eq!(
        rig.host.log().count_named("spawn"),
        1,
        "one spawn call only"
    );

    let rig = Rig::new();
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(
            HostErrorCode::BadArgument,
            "agent_name_taken: tw_demo_a",
        )),
        1,
    );
    let (report, store) = rig.launch("demo", rig.validate(&spec_for(&rig), false).unwrap());
    assert!(matches!(
        report.members.first().unwrap().outcome,
        LaunchOutcome::Refused(_)
    ));
    assert_eq!(rig.host.log().count_named("spawn"), 1, "not retried");
    assert_eq!(
        common::member_row(&store, "a").unwrap().state,
        teamwire_core::MemberState::Failed
    );
}

#[test]
fn a_spawn_timeout_that_started_the_agent_says_so_loudly() {
    use teamwire_core::{HostError, HostErrorCode};
    use teamwire_fakes::Fault;
    use teamwire_launch::LaunchOutcome;
    let rig = Rig::new();
    // The host starts the agent, then reports a timeout.
    rig.host.inject(
        "spawn",
        Fault::AcceptedButFail(HostError::new(HostErrorCode::Timeout, "too slow")),
        1,
    );
    let (report, _store) = rig.launch("demo", rig.validate(&spec_for(&rig), false).unwrap());
    let outcome = &report.members.first().unwrap().outcome;
    assert!(matches!(outcome, LaunchOutcome::Failed(_)), "{report:?}");
    let reason = format!("{outcome:?}");
    assert!(
        reason.contains("WARNING") && reason.contains("tw_demo_a"),
        "{reason}"
    );
    assert!(reason.contains("by hand"), "{reason}");
    assert_eq!(rig.host.log().count_named("spawn"), 1);

    // A timeout that left nothing behind says so too.
    let rig = Rig::new();
    rig.host.inject("spawn", Fault::Hang, 1);
    let (report, _store) = rig.launch("demo", rig.validate(&spec_for(&rig), false).unwrap());
    let text = format!("{:?}", report.members.first().unwrap().outcome);
    assert!(text.contains("does not show tw_demo_a"), "{text}");
}

fn spec_for(rig: &Rig) -> serde_json::Value {
    team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())])
}

#[test]
fn add_refused_when_close_lock_taken_after_validate() {
    use teamwire_launch::LaunchOutcome;
    let rig = Rig::new();
    let repo = rig.repo("proj");
    common::seed_team(&rig, "demo", |_, _| Ok(()));
    let spec = team_spec(
        "demo",
        &[
            worktree_member("first", "First", &repo),
            worktree_member("second", "Second", &repo),
        ],
    );
    let plan = rig.validate(&spec, true).unwrap();
    let mut other = teamwire_store::Store::open_existing(
        rig.home.path(),
        &teamwire_core::TeamName::parse("demo").unwrap(),
    )
    .unwrap();
    other
        .write(|tx| tx.begin_close(rig.clock.now(), teamwire_core::Pid::new(1)))
        .unwrap();
    let (report, store) = rig.launch("demo", plan);
    for member in &report.members {
        assert!(
            matches!(&member.outcome, LaunchOutcome::Refused(why) if why.contains("team_closed")),
            "{:?}",
            member.outcome
        );
    }
    assert_eq!(report.members.len(), 2);
    assert_eq!(rig.host.log().count(CallKind::Write), 0);
    // The one read is the check for names already alive, made before any member.
    let calls = rig.host.log().calls();
    assert!(
        calls.iter().all(|c| c.name == "live_agent_names"),
        "no spawn and no close: {calls:?}"
    );
    assert!(common::member_row(&store, "first").is_none());
    assert!(common::member_row(&store, "second").is_none());
    assert!(!repo.join(".worktrees/first").exists());
}

#[test]
fn branch_git_refuses_is_a_spec_error() {
    let rig = Rig::new();
    let repo = rig.repo("proj");
    // `main` is checked out in the repository itself.
    for branch in ["foo.lock", "a/.b", "a.lock/b", "main"] {
        let member = common::with(
            worktree_member("api", "API work", &repo),
            "branch",
            json!(branch),
        );
        let errors = rig
            .validate(&team_spec("demo", &[member]), false)
            .unwrap_err();
        let expect = if branch == "main" {
            "branch_checked_out"
        } else {
            "bad_branch"
        };
        assert_eq!(codes(&errors), [expect], "{branch}");
    }
    assert_eq!(rig.host.log().count_named("spawn"), 0);
}

#[test]
fn add_spec_without_title_uses_team_title() {
    let rig = Rig::new();
    common::seed_team(&rig, "demo", |_, _| Ok(()));
    let mut spec = team_spec("demo", &[pane_member("a", "Alpha", rig.work.path())]);
    spec.as_object_mut().unwrap().remove("title");
    let plan = rig.validate(&spec, true).unwrap();
    assert_eq!(plan.spec.title, "Seed", "the recorded team title is used");

    // A first launch still needs a title.
    let fresh = Rig::new();
    let errors = fresh.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["title_missing"]);
}

#[test]
fn bad_plugin_id_refused_naming_the_key() {
    let rig = Rig::new();
    let member = common::with(
        pane_member("a", "Alpha", rig.work.path()),
        "plugins",
        json!({"no-marketplace": false, "ok@x.y_z-1": true}),
    );
    let spec = common::with(
        team_spec("demo", &[member]),
        "plugins",
        json!({"@m": false}),
    );
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["bad_plugin_id", "bad_plugin_id"]);
    let text: String = errors.iter().map(|e| e.message.clone()).collect();
    assert!(text.contains("no-marketplace") && text.contains("\"@m\""));
    assert!(!text.contains("ok@x"));
}

#[test]
fn good_plugin_ids_pass() {
    let rig = Rig::new();
    let member = common::with(
        pane_member("a", "Alpha", rig.work.path()),
        "plugins",
        json!({"mattpocock-skills@claude-plugins-official": true}),
    );
    let spec = common::with(
        team_spec("demo", &[member]),
        "plugins",
        json!({"ponytail@ponytail": false}),
    );
    rig.validate(&spec, false).unwrap();
}

fn r(provider: &str, id: &str) -> serde_json::Value {
    json!({"provider": provider, "id": id})
}

#[test]
fn refs_name_their_provider_and_repeats_are_dropped() {
    let rig = Rig::new();
    let members = [pane_member("a", "Alpha", rig.work.path())];
    let ok = common::with(
        team_spec("demo", &members),
        "refs",
        json!([
            r("linear", "GKA-580"),
            r("github", "#3772"),
            r("github", "knit-insights/teamwire#12"),
            r("linear", "GKA-580"),
            r("a_1", &"x".repeat(64)),
            r("abcdefghijklmnop", "ok"),
            // The same id under another provider is another ref.
            r("jira", "GKA-580"),
        ]),
    );
    let plan = rig.validate(&ok, false).unwrap();
    let shown: Vec<String> = plan.spec.refs.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown.len(),
        6,
        "only the exact repeat is dropped: {shown:?}"
    );
    assert_eq!(shown.first().map(String::as_str), Some("linear:GKA-580"));
    assert_eq!(shown.get(1).map(String::as_str), Some("github:#3772"));
    assert_eq!(shown.last().map(String::as_str), Some("jira:GKA-580"));
}

#[test]
fn a_ref_without_a_provider_or_with_a_bad_one_is_refused() {
    let rig = Rig::new();
    let members = [pane_member("a", "Alpha", rig.work.path())];
    let spec = |refs: serde_json::Value| common::with(team_spec("demo", &members), "refs", refs);

    // A bare string, a missing field, a field of the wrong type and an
    // unknown field fail parsing, each as its own `bad_ref`.
    for (bad, words) in [
        (json!(["GKA-580"]), "assumes no provider"),
        (json!([{"id": "GKA-580"}]), "\"provider\""),
        (json!([{"provider": "linear"}]), "\"id\""),
        (json!([{"provider": 1, "id": "x"}]), "\"provider\""),
        (
            json!([{"provider": "linear", "id": "x", "url": "u"}]),
            "unknown field",
        ),
    ] {
        let errors = parse_spec(&spec(bad.clone()).to_string()).unwrap_err();
        assert_eq!(codes(&errors), ["bad_ref"], "{bad}");
        let message = errors.first().map_or("", |e| e.message.as_str());
        assert!(message.contains(words), "{bad}: {message}");
    }

    // A name of the wrong shape, and ids that break the old rules, fail `validate`.
    let bad = spec(json!([
        r("Linear", "A-1"),
        r("", "A-1"),
        r("1x", "A-1"),
        r("has-hyphen", "A-1"),
        r("abcdefghijklmnopq", "A-1"),
        r("linear", "GKA 580"),
        r("linear", ""),
        r("linear", &"x".repeat(65)),
        r("linear", "tab\there"),
        r("linear", "nul\u{1}"),
        r("linear", "ok-1"),
    ]));
    let errors = rig.validate(&bad, false).unwrap_err();
    assert_eq!(codes(&errors), ["bad_ref"; 10]);
    assert!(errors.iter().all(|e| e.field.as_deref() == Some("refs")));
    assert!(rig.home_is_empty());
}

#[test]
fn more_than_twenty_refs_refused() {
    let rig = Rig::new();
    let members = [pane_member("a", "Alpha", rig.work.path())];
    let many = |n: usize| {
        (0..n)
            .map(|n| r("linear", &format!("T-{n}")))
            .collect::<Vec<_>>()
    };
    let spec = common::with(team_spec("demo", &members), "refs", json!(many(20)));
    rig.validate(&spec, false).unwrap();

    let spec = common::with(team_spec("demo", &members), "refs", json!(many(21)));
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["too_many_refs"]);
}
