//! The launch half of a handoff against the fake host and the fake harness:
//! a fresh session in the member's seat, with the per-run files launch wrote.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
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

use common::{Rig, member_row, pane_member, role_events, team_spec};
use teamwire_core::{Address, Clock, Host, HostError, HostErrorCode, Pid, Role, SessionId};
use teamwire_fakes::{FakeHarness, Fault};
use teamwire_launch::{Handoff, HandoffError, open_handoff, plan_handoff};
use teamwire_store::{Ctx, Store};

const T: Duration = Duration::from_secs(5);

fn role(text: &str) -> Role {
    Role::parse(text).unwrap()
}

/// Team `demo` with one running pane member `api`.
fn running_team(rig: &Rig) -> Store {
    rig.harness
        .set_session_cwd(SessionId::parse("lead-session").unwrap(), rig.work.path());
    let spec = team_spec("demo", &[pane_member("api", "Api", rig.work.path())]);
    let (report, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    assert_eq!(
        report.members[0].outcome,
        teamwire_launch::LaunchOutcome::Started
    );
    store
}

/// Closes `api`'s place, as the close half of a handoff does first.
fn close_old_place(rig: &Rig, store: &Store) {
    let place = member_row(store, "api").unwrap().place.unwrap();
    rig.host
        .close(&place, &Address::parse("tw_demo_api").unwrap(), T)
        .unwrap();
    rig.host.log().clear();
}

fn ctx<'a>(rig: &'a Rig, store: &'a mut Store) -> Ctx<'a> {
    Ctx {
        home: rig.home.path(),
        store,
        registry: &rig.registry,
        platform: &*rig.platform,
        clock: &rig.clock as &dyn Clock,
        config: &rig.config,
    }
}

fn hand_off(rig: &Rig, store: &mut Store) -> Result<Handoff, String> {
    let mut ctx = ctx(rig, store);
    let plan = plan_handoff(&ctx, &role("api"), None).map_err(|e| e.to_string())?;
    open_handoff(&mut ctx, &plan)
}

fn host_calls(rig: &Rig, name: &str) -> Vec<String> {
    rig.host
        .log()
        .calls()
        .into_iter()
        .filter(|c| c.name == name)
        .map(|c| c.detail)
        .collect()
}

fn document(rig: &Rig) -> std::path::PathBuf {
    rig.home.path().join("teams/demo/reports/api.handoff.md")
}

/// The handoff document, written now: after the fake clock's start, so in
/// the generation being replaced.
fn write_document(rig: &Rig) {
    std::fs::create_dir_all(document(rig).parent().unwrap()).unwrap();
    std::fs::write(document(rig), "# api handoff\n").unwrap();
}

fn settings(rig: &Rig) -> Vec<u8> {
    let path = rig.home.path().join("teams/demo/members/api/settings.json");
    std::fs::read(path).unwrap()
}

#[test]
fn a_handoff_starts_a_new_session_with_the_old_files_and_records_it() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    let old = before.session_id.clone().unwrap();
    let files = settings(&rig);
    write_document(&rig);
    close_old_place(&rig, &store);

    let done = hand_off(&rig, &mut store).unwrap();

    let after = member_row(&store, "api").unwrap();
    let new = after.session_id.clone().unwrap();
    assert_ne!(new, old, "a fresh session");
    assert_eq!(after.place.as_ref(), Some(&done.place));
    assert_eq!(after.generation, before.generation + 1);
    assert_eq!(done.generation, after.generation);
    let (title, argv, env) = rig.host.spawned_with(&done.place).unwrap();
    assert_eq!(title, "Api");
    assert_eq!(&argv[..3], ["fake-claude", "--session-id", new.as_str()]);
    assert!(
        argv.iter()
            .any(|a| a.ends_with("members/api/settings.json"))
    );
    assert!(env.is_empty(), "the identity is in the per-run files");
    assert_eq!(settings(&rig), files, "the per-run files are not written");
    assert_eq!(rig.host.cwd_of(&done.place).unwrap(), rig.work.path());

    let events = role_events(&store, "api");
    let (_, data) = events.iter().rfind(|(k, _)| k == "member.handoff").unwrap();
    assert_eq!(data["old_session"], old.as_str());
    assert_eq!(data["new_session"], new.as_str());

    let lines = rig.host.submitted(&done.place);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("teams/demo/briefs/api.md"), "{lines:?}");
    assert!(
        lines[0].contains("teams/demo/reports/api.handoff.md"),
        "{lines:?}"
    );
    assert!(done.warnings.is_empty(), "{:?}", done.warnings);
}

#[test]
fn a_failed_spawn_keeps_the_old_session_and_names_reopen() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    close_old_place(&rig, &store);
    rig.host.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::Other, "boom")),
        1,
    );
    let why = hand_off(&rig, &mut store).unwrap_err();
    assert!(why.contains("boom"), "{why}");
    assert!(why.contains("`teamwire reopen demo --role api`"), "{why}");
    assert_eq!(member_row(&store, "api").unwrap(), before);
}

#[test]
fn a_failed_trust_step_closes_the_new_tab_and_records_nothing() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    close_old_place(&rig, &store);
    rig.host.inject(
        "read_tail",
        Fault::Fail(HostError::new(HostErrorCode::Other, "screen unreadable")),
        1,
    );
    let why = hand_off(&rig, &mut store).unwrap_err();
    assert!(why.contains("the new tab was closed"), "{why}");
    assert!(why.contains("teamwire reopen demo --role api"), "{why}");
    assert_eq!(host_calls(&rig, "spawn").len(), 1);
    assert_eq!(host_calls(&rig, "close").len(), 1, "the new tab was closed");
    assert_eq!(member_row(&store, "api").unwrap(), before);
    assert!(
        !role_events(&store, "api")
            .iter()
            .any(|(k, _)| k == "member.handoff")
    );
}

#[test]
fn a_harness_that_picks_its_own_session_is_recorded_with_none() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    close_old_place(&rig, &store);
    let mut caps = FakeHarness::caps();
    caps.choose_session_id = false;
    rig.harness.set_caps(caps);

    let done = hand_off(&rig, &mut store).unwrap();

    let (_, argv, _) = rig.host.spawned_with(&done.place).unwrap();
    assert!(!argv.iter().any(|a| a == "--session-id"), "{argv:?}");
    assert_eq!(member_row(&store, "api").unwrap().session_id, None);
    let events = role_events(&store, "api");
    let (_, data) = events.iter().rfind(|(k, _)| k == "member.handoff").unwrap();
    assert!(data["new_session"].is_null(), "{data}");
}

#[test]
fn planning_refuses_a_closing_team_and_an_unknown_member_and_writes_nothing() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    {
        let ctx = ctx(&rig, &mut store);
        let why = plan_handoff(&ctx, &role("nobody"), None).unwrap_err();
        assert!(
            matches!(&why, HandoffError::Refused(w) if w.contains("no member nobody")),
            "{why}"
        );
    }
    let now = rig.clock.now();
    store.write(|tx| tx.begin_close(now, Pid::new(1))).unwrap();
    let ctx = ctx(&rig, &mut store);
    let why = plan_handoff(&ctx, &role("api"), None).unwrap_err();
    assert!(
        matches!(&why, HandoffError::Refused(w) if w.contains("only an open team can hand off")),
        "{why}"
    );
    assert!(
        host_calls(&rig, "spawn").len() == 1,
        "only the launch spawned"
    );
}

#[test]
fn a_session_its_start_hook_recorded_before_the_handoff_write_is_kept() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    close_old_place(&rig, &store);
    let mut caps = FakeHarness::caps();
    caps.choose_session_id = false;
    rig.harness.set_caps(caps);
    let hooked = SessionId::parse("codex-new").unwrap();
    let mut ctx = ctx(&rig, &mut store);
    let plan = plan_handoff(&ctx, &role("api"), None).unwrap();
    // The new session's start hook runs while the tab opens.
    ctx.store
        .write(|tx| tx.set_member_session(&role("api"), &hooked))
        .unwrap();
    open_handoff(&mut ctx, &plan).unwrap();
    assert_eq!(member_row(&store, "api").unwrap().session_id, Some(hooked));
}

#[test]
fn a_current_document_is_named_and_a_missing_or_old_one_is_replaced_by_the_report() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let plan_of = |store: &mut Store| {
        let ctx = ctx(&rig, store);
        let plan = plan_handoff(&ctx, &role("api"), None).unwrap();
        (plan.first_line.clone(), plan.warnings.clone())
    };

    let (line, warnings) = plan_of(&mut store);
    assert!(
        warnings.len() == 1 && warnings[0].contains("is missing"),
        "{warnings:?}"
    );
    assert!(!line.contains("api.handoff.md"), "{line}");
    assert!(line.contains("teams/demo/reports/api.md"), "{line}");
    assert!(line.contains("teams/demo/briefs/api.md"), "{line}");

    write_document(&rig);
    let (line, warnings) = plan_of(&mut store);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(line.contains("teams/demo/reports/api.handoff.md"), "{line}");

    // A document from 2020, before this generation started.
    let old = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    std::fs::File::options()
        .write(true)
        .open(document(&rig))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let (line, warnings) = plan_of(&mut store);
    assert!(
        warnings.len() == 1 && warnings[0].contains("written before this session started"),
        "{warnings:?}"
    );
    assert!(!line.contains("api.handoff.md"), "{line}");
    assert!(line.contains("teams/demo/reports/api.md"), "{line}");
}

#[test]
fn a_failed_trust_step_puts_back_the_old_session_a_start_hook_replaced() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let old = member_row(&store, "api").unwrap().session_id.unwrap();
    close_old_place(&rig, &store);
    let mut caps = FakeHarness::caps();
    caps.choose_session_id = false;
    rig.harness.set_caps(caps);
    rig.host.inject(
        "read_tail",
        Fault::Fail(HostError::new(HostErrorCode::Other, "screen unreadable")),
        1,
    );
    let mut ctx = ctx(&rig, &mut store);
    let plan = plan_handoff(&ctx, &role("api"), None).unwrap();
    // The new Codex session's start hook records it while the tab opens.
    ctx.store
        .write(|tx| tx.set_member_session(&role("api"), &SessionId::parse("codex-new").unwrap()))
        .unwrap();
    let why = open_handoff(&mut ctx, &plan).unwrap_err();
    assert!(
        why.contains(&format!("The old session {old} is still recorded")),
        "{why}"
    );
    assert_eq!(member_row(&store, "api").unwrap().session_id, Some(old));
}

#[test]
fn a_store_that_cannot_be_read_is_a_store_error() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    store
        .write(|tx| {
            Ok(tx.connection().execute(
                "UPDATE members SET harness = 'Not A Harness!' WHERE role = 'api'",
                (),
            )?)
        })
        .unwrap();
    let ctx = ctx(&rig, &mut store);
    let why = plan_handoff(&ctx, &role("api"), None).unwrap_err();
    assert!(matches!(why, HandoffError::Store(_)), "{why}");
}
