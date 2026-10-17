//! `reopen` against the fake host and the fake harness: the lead first, then
//! every started member whose session is gone.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::redundant_closure_for_method_calls,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use std::time::Duration;

use common::{Rig, member_row, pane_member, team_spec, with};
use serde_json::json;
use teamwire_core::{
    Clock, Host, Liveness, MemberState, Pid, PlaceState, Role, SessionId, SessionKind,
};
use teamwire_launch::{ReopenError, ReopenOpts, ReopenOutcome, ReopenReport, reopen};
use teamwire_store::{Ctx, Store};

const T: Duration = Duration::from_secs(5);

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn role(text: &str) -> Role {
    Role::parse(text).unwrap()
}

/// Launches team `demo` with a pane member for each role, as a lead with a
/// recorded folder would, then closes every tab: all sessions are gone.
fn closed_team(rig: &Rig, roles: &[&str]) -> Store {
    rig.harness
        .set_session_cwd(sid("lead-session"), rig.work.path());
    let members: Vec<_> = roles
        .iter()
        .map(|r| pane_member(r, r, rig.work.path()))
        .collect();
    let spec = team_spec("demo", &members);
    let (report, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    assert!(
        report
            .members
            .iter()
            .all(|m| m.outcome == teamwire_launch::LaunchOutcome::Started)
    );
    for r in roles {
        close_tab(rig, &store, r);
    }
    rig.host.log().clear();
    store
}

fn close_tab(rig: &Rig, store: &Store, role_name: &str) {
    let place = member_row(store, role_name).unwrap().place.unwrap();
    rig.host.close(&place, T).unwrap();
}

fn run(rig: &Rig, store: &mut Store, opts: &ReopenOpts) -> Result<ReopenReport, ReopenError> {
    let mut ctx = Ctx {
        home: rig.home.path(),
        store,
        registry: &rig.registry,
        platform: &*rig.platform,
        clock: &rig.clock as &dyn Clock,
        config: &rig.config,
    };
    reopen(&mut ctx, opts)
}

fn all() -> ReopenOpts {
    ReopenOpts::default()
}

fn spawned(rig: &Rig) -> Vec<String> {
    rig.host
        .log()
        .calls()
        .into_iter()
        .filter(|c| c.name == "spawn")
        .map(|c| c.detail)
        .collect()
}

fn outcome<'a>(report: &'a ReopenReport, who: &str) -> &'a ReopenOutcome {
    &report
        .members
        .iter()
        .find(|(r, _)| r.as_str() == who)
        .unwrap()
        .1
}

#[test]
fn a_dead_member_is_reopened_with_its_resume_argv_in_its_folder() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api"]);
    let session = member_row(&store, "api").unwrap().session_id.unwrap();
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(outcome(&report, "api"), &ReopenOutcome::Reopened);
    let place = member_row(&store, "api").unwrap().place.unwrap();
    let (title, argv, env) = rig.host.spawned_with(&place).unwrap();
    assert_eq!(title, "api");
    assert_eq!(&argv[..3], ["fake-claude", "--resume", session.as_str()]);
    assert!(env.is_empty(), "the identity is in the per-run files");
    assert_eq!(rig.host.cwd_of(&place).unwrap(), rig.work.path());
    assert_eq!(
        member_row(&store, "api").unwrap().session_id,
        Some(session),
        "the session id does not change"
    );
}

#[test]
fn a_member_whose_session_runs_or_is_unknown_is_skipped() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["up", "unsure", "gone"]);
    let session = |r: &str| member_row(&store, r).unwrap().session_id.unwrap();
    rig.harness
        .add_session(session("up"), vec![], Liveness::Alive(PlaceState::Idle));
    rig.harness
        .add_session(session("unsure"), vec![], Liveness::Unknown);
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(
        outcome(&report, "up"),
        &ReopenOutcome::Skipped("already open".to_owned())
    );
    assert!(
        matches!(outcome(&report, "unsure"), ReopenOutcome::Skipped(why) if why.contains("not known"))
    );
    assert_eq!(outcome(&report, "gone"), &ReopenOutcome::Reopened);
    assert_eq!(spawned(&rig), ["tw_demo_lead", "tw_demo_gone"]);
}

#[test]
fn removed_and_failed_members_are_skipped() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["old", "new"]);
    let now = rig.clock.now();
    store
        .write(|tx| tx.remove_member(now, &role("old"), None, &json!({})))
        .unwrap();
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(
        outcome(&report, "old"),
        &ReopenOutcome::Skipped("removed".to_owned())
    );
    assert_eq!(outcome(&report, "new"), &ReopenOutcome::Reopened);
}

#[test]
fn role_picks_one_member_and_leaves_the_lead_alone() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api", "web"]);
    let only = ReopenOpts {
        role: Some(role("web")),
        dry_run: false,
    };
    let report = run(&rig, &mut store, &only).unwrap();
    assert_eq!(report.lead, None);
    assert_eq!(report.members.len(), 1);
    assert_eq!(spawned(&rig), ["tw_demo_web"]);
    let missing = ReopenOpts {
        role: Some(role("nobody")),
        dry_run: false,
    };
    assert_eq!(
        run(&rig, &mut store, &missing).unwrap_err(),
        ReopenError::NoMember(role("nobody"))
    );
}

#[test]
fn a_closing_or_closed_team_is_refused() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api"]);
    let now = rig.clock.now();
    store.write(|tx| tx.begin_close(now, Pid::new(1))).unwrap();
    assert!(matches!(
        run(&rig, &mut store, &all()),
        Err(ReopenError::NotOpen { .. })
    ));
    assert!(spawned(&rig).is_empty());
}

#[test]
fn a_dry_run_spawns_nothing_and_writes_nothing() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api"]);
    let seq = store.read(|tx| tx.last_event_seq()).unwrap();
    let before = member_row(&store, "api").unwrap().place;
    let dry = ReopenOpts {
        role: None,
        dry_run: true,
    };
    let report = run(&rig, &mut store, &dry).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.lead, Some(ReopenOutcome::Reopened));
    assert_eq!(outcome(&report, "api"), &ReopenOutcome::Reopened);
    assert!(spawned(&rig).is_empty());
    assert_eq!(store.read(|tx| tx.last_event_seq()).unwrap(), seq);
    assert_eq!(member_row(&store, "api").unwrap().place, before);
    assert!(rig.platform.inner.hold_events().is_empty());
}

#[test]
fn a_member_without_its_files_or_folder_is_refused_with_the_reason() {
    let rig = Rig::new();
    let elsewhere = rig.work.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    rig.harness
        .set_session_cwd(sid("lead-session"), rig.work.path());
    let spec = team_spec(
        "demo",
        &[
            pane_member("nofile", "nofile", rig.work.path()),
            pane_member("nodir", "nodir", &elsewhere),
        ],
    );
    let (_, mut store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    for r in ["nofile", "nodir"] {
        close_tab(&rig, &store, r);
    }
    rig.host.log().clear();
    let settings = rig
        .home
        .path()
        .join("teams/demo/members/nofile/settings.json");
    std::fs::remove_file(&settings).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
    let report = run(&rig, &mut store, &all()).unwrap();
    let why = |who| {
        let ReopenOutcome::Refused(why) = outcome(&report, who) else {
            unreachable!("{who} was not refused");
        };
        why.clone()
    };
    assert!(
        why("nodir").contains("elsewhere is gone"),
        "{}",
        why("nodir")
    );
    assert!(
        why("nofile").contains("settings.json is missing"),
        "{}",
        why("nofile")
    );
    assert_eq!(spawned(&rig), ["tw_demo_lead"]);
}

#[test]
fn a_member_launched_without_a_model_is_not_warned_about() {
    let rig = Rig::new();
    let with_model = with(
        with(
            pane_member("a", "a", rig.work.path()),
            "model",
            json!("opus"),
        ),
        "effort",
        json!("high"),
    );
    let plain = pane_member("b", "b", rig.work.path());
    rig.harness
        .set_session_cwd(sid("lead-session"), rig.work.path());
    let spec = team_spec("demo", &[with_model, plain]);
    let (_, mut store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    for r in ["a", "b"] {
        close_tab(&rig, &store, r);
    }
    rig.host.log().clear();
    rig.harness.set_interactive_caller(sid("other"));
    let report = run(&rig, &mut store, &all()).unwrap();
    // A recorded `null` means the member ran on the default.
    assert!(
        report.warnings.iter().all(|w| !w.contains("no model")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_member_an_older_build_started_is_warned_about() {
    let rig = Rig::new();
    let dir = rig.home.path().join("teams/demo/members/old");
    std::fs::create_dir_all(&dir).unwrap();
    common::write(&dir.join("settings.json"), "{}");
    common::seed_team(&rig, "demo", |tx, at| {
        let old = role("old");
        tx.insert_member(at, &common::new_member("old", "old", rig.work.path()))?;
        tx.set_member_session(&old, &sid("old-session"))?;
        // An older build wrote no model or effort key.
        tx.set_member_state(at, &old, MemberState::Started, &json!({}))
    });
    let mut store = Store::open(
        rig.home.path(),
        &teamwire_core::TeamName::parse("demo").unwrap(),
    )
    .unwrap();
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(outcome(&report, "old"), &ReopenOutcome::Reopened);
    let unrecorded: Vec<_> = report
        .warnings
        .iter()
        .filter(|w| w.contains("no model or effort"))
        .collect();
    assert_eq!(unrecorded.len(), 1, "{:?}", report.warnings);
    assert!(unrecorded[0].starts_with("old has no model or effort"));
}

#[test]
fn a_host_that_fails_to_spawn_fails_that_member_only() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api", "web"]);
    rig.host.inject(
        "spawn",
        teamwire_fakes::Fault::Fail(teamwire_core::HostError::new(
            teamwire_core::HostErrorCode::Other,
            "boom",
        )),
        1,
    );
    let report = run(
        &rig,
        &mut store,
        &ReopenOpts {
            role: None,
            dry_run: false,
        },
    )
    .unwrap();
    // The lead's spawn takes the fault; both members still open.
    assert!(matches!(report.lead, Some(ReopenOutcome::Failed(_))));
    assert_eq!(outcome(&report, "api"), &ReopenOutcome::Reopened);
    assert_eq!(outcome(&report, "web"), &ReopenOutcome::Reopened);
    assert!(report.has_problem());
}

#[test]
fn the_lead_comes_back_first_in_its_folder_under_its_own_name() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api"]);
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(report.lead, Some(ReopenOutcome::Reopened));
    assert_eq!(spawned(&rig), ["tw_demo_lead", "tw_demo_api"]);
    let lead = rig
        .host
        .log()
        .calls()
        .into_iter()
        .position(|c| c.name == "spawn" && c.detail == "tw_demo_lead")
        .unwrap();
    assert_eq!(
        lead,
        rig.host
            .log()
            .calls()
            .iter()
            .position(|c| c.name == "spawn")
            .unwrap()
    );
}

#[test]
fn a_running_or_background_lead_is_skipped() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api"]);
    rig.harness.add_session(
        sid("lead-session"),
        vec![],
        Liveness::Alive(PlaceState::Working),
    );
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(
        report.lead,
        Some(ReopenOutcome::Skipped("already open".to_owned()))
    );
    assert_eq!(spawned(&rig), ["tw_demo_api"]);

    let rig = Rig::new();
    let mut store = seeded_lead(&rig, "fake", SessionKind::Background);
    let report = run(&rig, &mut store, &all()).unwrap();
    assert!(matches!(report.lead, Some(ReopenOutcome::Skipped(why)) if why.contains("background")));
    assert!(spawned(&rig).is_empty());
}

#[test]
fn a_lead_with_no_folder_found_is_refused_with_the_manual_command() {
    let rig = Rig::new();
    // No folder recorded and the harness does not know the session.
    let spec = team_spec("demo", &[pane_member("api", "api", rig.work.path())]);
    let (_, mut store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    let report = run(&rig, &mut store, &all()).unwrap();
    let Some(ReopenOutcome::Refused(why)) = &report.lead else {
        unreachable!("{:?}", report.lead)
    };
    assert!(why.contains("fake-claude --resume lead-session"), "{why}");
    assert!(report.has_problem());
    assert!(!spawned(&rig).contains(&"tw_demo_lead".to_owned()));
}

#[test]
fn the_harness_fills_in_a_folder_an_older_team_never_recorded() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("api", "api", rig.work.path())]);
    let (_, mut store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    rig.harness
        .set_session_cwd(sid("lead-session"), rig.work.path());
    let report = run(&rig, &mut store, &all()).unwrap();
    assert_eq!(report.lead, Some(ReopenOutcome::Reopened));
}

/// A team whose lead was recorded by hand: launch refuses a background lead
/// and a lead on host `none`, but an older team can have either.
fn seeded_lead(rig: &Rig, host: &'static str, kind: SessionKind) -> Store {
    let name = teamwire_core::TeamName::parse("demo").unwrap();
    let mut store = Store::open(rig.home.path(), &name).unwrap();
    let at = rig.clock.now();
    store
        .write(|tx| {
            tx.create_team_with(
                at,
                &teamwire_store::NewTeam {
                    name: name.clone(),
                    title: "Demo".to_owned(),
                    lead_harness: teamwire_core::HarnessId::from_static(
                        teamwire_fakes::FakeHarness::ID,
                    ),
                    lead_host: teamwire_core::HostId::from_static(host),
                    lead_route: teamwire_core::LeadRoute::Sink,
                    lead_session: Some((sid("lead-session"), kind)),
                },
                &json!({ "lead_cwd": "/work/my lead" }),
            )
            .map(|_| ())
        })
        .unwrap();
    store
}

#[test]
fn a_lead_on_host_none_is_refused_with_the_manual_command() {
    let rig = Rig::new();
    let mut store = seeded_lead(&rig, "none", SessionKind::Interactive);
    let report = run(&rig, &mut store, &all()).unwrap();
    let Some(ReopenOutcome::Refused(why)) = &report.lead else {
        unreachable!("{:?}", report.lead)
    };
    assert!(
        why.contains("cd '/work/my lead' && fake-claude --resume lead-session"),
        "{why}"
    );
    assert!(spawned(&rig).is_empty());
}

#[test]
fn after_the_lead_is_up_the_sleep_hold_is_taken() {
    let rig = Rig::new();
    let mut store = closed_team(&rig, &["api"]);
    // The lead's session is gone but its process list names the process the
    // hold waits on, as it would once the reopened lead runs.
    rig.harness
        .add_session(sid("lead-session"), vec![Pid::new(4242)], Liveness::Dead);
    run(&rig, &mut store, &all()).unwrap();
    assert_eq!(rig.platform.inner.hold_events().len(), 1);
    let hold = store.read(|tx| tx.team()).unwrap().unwrap().sleep_hold;
    assert!(hold.is_some());
}

#[test]
fn a_hold_that_cannot_be_taken_is_a_warning_not_a_failure() {
    let rig = Rig::without_hold();
    let mut store = closed_team(&rig, &["api"]);
    let report = run(&rig, &mut store, &all()).unwrap();
    assert!(!report.has_problem());
    assert!(report.warnings.iter().any(|w| w.contains("no sleep hold")));
    assert!(member_row(&store, "api").unwrap().state == MemberState::Started);
}
