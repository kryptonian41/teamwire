//! The order of a handoff's steps in `hand_off`, against the fake host and
//! the fake harness, with stand-ins for close's checks and place close.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "helper functions in an integration-test file unwrap, index and panic; a failure there fails the test"
)]
#![expect(
    clippy::unnecessary_wraps,
    reason = "a stand-in close that never fails still has the type `PlaceCloser::close` asks for"
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
use teamwire_core::{Address, Clock, HostError, HostErrorCode, Member, Role, SessionId, TeamName};
use teamwire_fakes::{FakeHarness, Fault};
use teamwire_launch::{HandoffError, PlaceCloser, Said, hand_off};
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
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    store
}

fn no_refusals(_: &Ctx<'_>, _: &Member) -> Vec<String> {
    Vec::new()
}

/// Closes the old place through the member's host, as close does.
fn close_place(ctx: &Ctx<'_>, team: &TeamName, member: &Member) -> Result<Option<String>, String> {
    let place = member.place.as_ref().unwrap();
    let host = ctx.registry.host(&member.host).unwrap();
    host.close(place, &Address::of(team, &member.role), T)
        .unwrap();
    Ok(Some(format!("{}'s place {} closed", member.role, place.id)))
}

const CLOSES: PlaceCloser = PlaceCloser {
    refusals: no_refusals,
    close: close_place,
};

/// Runs `hand_off` for `api` and returns what it said with its result.
fn run(rig: &Rig, store: &mut Store, closer: PlaceCloser) -> (Vec<Said>, Result<(), HandoffError>) {
    let mut ctx = Ctx {
        home: rig.home.path(),
        store,
        registry: &rig.registry,
        platform: &*rig.platform,
        clock: &rig.clock as &dyn Clock,
        config: &rig.config,
    };
    let mut said = Vec::new();
    let result = hand_off(&mut ctx, &role("api"), None, closer, false, &mut |s| {
        said.push(s);
    });
    (said, result)
}

fn host_calls(rig: &Rig, name: &str) -> usize {
    rig.host
        .log()
        .calls()
        .iter()
        .filter(|c| c.name == name)
        .count()
}

#[test]
fn a_refusal_stops_before_any_close() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    rig.host.log().clear();
    let closer = PlaceCloser {
        refusals: |_, _| vec!["api is busy".to_owned(), "api is dirty".to_owned()],
        close: |_, _, _| panic!("a refused handoff closed the place"),
    };
    let (said, result) = run(&rig, &mut store, closer);
    assert_eq!(
        result,
        Err(HandoffError::Refused(
            "api is busy; api is dirty".to_owned()
        ))
    );
    assert!(said.is_empty(), "{said:?}");
    assert_eq!(host_calls(&rig, "close"), 0);
    assert_eq!(host_calls(&rig, "spawn"), 0);
    assert_eq!(member_row(&store, "api").unwrap(), before);
}

#[test]
fn a_failed_place_close_stops_before_the_spawn_and_names_reopen() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    rig.host.log().clear();
    let closer = PlaceCloser {
        refusals: no_refusals,
        close: |_, _, _| Err("api's place p1 closed, but process(es) 42 still run".to_owned()),
    };
    let (said, result) = run(&rig, &mut store, closer);
    let Err(HandoffError::Partial(why)) = result else {
        panic!("{result:?}");
    };
    assert!(why.starts_with("api's place p1 closed, but"), "{why}");
    assert!(why.contains("No new session was opened"), "{why}");
    assert!(why.contains("`teamwire reopen demo --role api`"), "{why}");
    assert_eq!(host_calls(&rig, "spawn"), 0);
    assert_eq!(
        said.first(),
        Some(&Said::Line(
            "api: started, idle and committed, so it can hand off".to_owned()
        ))
    );
    assert_eq!(member_row(&store, "api").unwrap(), before);
}

/// `close_place`, then the new session's start hook recording an id of its
/// own while the tab opens, as the hook of a harness that picks its own
/// session id does.
fn close_place_then_hook(
    ctx: &Ctx<'_>,
    team: &TeamName,
    member: &Member,
) -> Result<Option<String>, String> {
    let said = close_place(ctx, team, member);
    Store::open_existing(ctx.home, team)
        .unwrap()
        .write(|tx| tx.set_member_session(&member.role, &SessionId::parse("hooked-new").unwrap()))
        .unwrap();
    said
}

#[test]
fn a_failure_after_the_spawn_closes_the_new_tab_and_restores_the_old_session() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    let old = before.session_id.clone().unwrap();
    let mut caps = FakeHarness::caps();
    caps.choose_session_id = false;
    rig.harness.set_caps(caps);
    rig.host.log().clear();
    rig.host.inject(
        "read_tail",
        Fault::Fail(HostError::new(HostErrorCode::Other, "screen unreadable")),
        1,
    );
    let closer = PlaceCloser {
        refusals: no_refusals,
        close: close_place_then_hook,
    };
    let (_, result) = run(&rig, &mut store, closer);
    let Err(HandoffError::Partial(why)) = result else {
        panic!("{result:?}");
    };
    assert!(why.contains("the new tab was closed"), "{why}");
    assert!(
        why.contains(&format!("The old session {old} is still recorded")),
        "{why}"
    );
    assert_eq!(host_calls(&rig, "spawn"), 1);
    assert_eq!(
        host_calls(&rig, "close"),
        2,
        "the old place, then the new tab"
    );
    let after = member_row(&store, "api").unwrap();
    assert_eq!(after.session_id, Some(old));
    assert_eq!(after.generation, before.generation);
    assert!(
        !role_events(&store, "api")
            .iter()
            .any(|(k, _)| k == "member.handoff")
    );
}

#[test]
fn a_handoff_closes_then_opens_and_records_it() {
    let rig = Rig::new();
    let mut store = running_team(&rig);
    let before = member_row(&store, "api").unwrap();
    let old_place = before.place.clone().unwrap();
    rig.host.log().clear();
    let (said, result) = run(&rig, &mut store, CLOSES);
    result.unwrap();

    let after = member_row(&store, "api").unwrap();
    let new = after.session_id.clone().unwrap();
    let place = after.place.clone().unwrap();
    assert_ne!(Some(&new), before.session_id.as_ref());
    assert_eq!(after.generation, before.generation + 1);
    let calls: Vec<&str> = rig
        .host
        .log()
        .calls()
        .into_iter()
        .filter(|c| c.name == "close" || c.name == "spawn")
        .map(|c| c.name)
        .collect();
    assert_eq!(calls, ["close", "spawn"], "the old place closes first");
    assert!(
        role_events(&store, "api")
            .iter()
            .any(|(k, _)| k == "member.handoff")
    );
    // No handoff document was written, so the plan warns once.
    let lines: Vec<String> = said
        .iter()
        .filter_map(|s| match s {
            Said::Line(l) => Some(l.clone()),
            Said::Warning(_) => None,
        })
        .collect();
    assert_eq!(
        lines,
        [
            "api: started, idle and committed, so it can hand off".to_owned(),
            format!("api's place {} closed", old_place.id),
            format!(
                "api: opened session {new} in place {}, trust step done",
                place.id
            ),
            format!("api: handoff recorded, generation {}", after.generation),
            "api: first line sent, pointing at the brief and the handoff document".to_owned(),
        ]
    );
    assert!(
        matches!(&said[1], Said::Warning(w) if w.starts_with("warning: api: the handoff document")),
        "{said:?}"
    );
}
