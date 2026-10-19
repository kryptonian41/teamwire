//! Unit-level tests of the in-process fake `host`.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::time::Duration;

use teamwire_core::{
    Address, Host, HostCaps, HostError, HostErrorCode, HostId, Key, Occupant, Pid, Place,
    PlaceState, SessionId, SpawnRequest, sanitize::one_line,
};
use teamwire_fakes::{CallKind, FakeHost, Fault};

const T: Duration = Duration::from_secs(1);

fn req(name: &str) -> SpawnRequest {
    SpawnRequest {
        name: Address::parse(name).unwrap(),
        title: "title".into(),
        cwd: "/work".into(),
        argv: vec!["claude".into()],
        env: vec![],
    }
}

fn occ(session: &str, pid: u32) -> Occupant {
    Occupant {
        session: Some(SessionId::parse(session).unwrap()),
        pids: vec![Pid::new(pid)],
    }
}

#[test]
fn spawn_list_state_close_round_trip() {
    let h = FakeHost::new();
    let p = h.spawn(&req("tw_demo_api"), T).unwrap();
    assert_eq!(h.list(T).unwrap(), std::slice::from_ref(&p));
    assert_eq!(h.live_agent_names(T).unwrap(), ["tw_demo_api"]);
    assert_eq!(h.state(&p, T).unwrap(), PlaceState::Idle);
    let name = Address::parse("tw_demo_api").unwrap();
    h.close(&p, &name, T).unwrap();
    assert!(h.list(T).unwrap().is_empty());
    assert_eq!(h.state(&p, T).unwrap_err().code, HostErrorCode::NotFound);
    assert_eq!(
        h.close(&p, &name, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
}

#[test]
fn close_leaves_a_place_that_holds_another_name() {
    let h = FakeHost::new();
    let p = h.spawn(&req("tw_other_web"), T).unwrap();
    let mine = Address::parse("tw_demo_api").unwrap();
    assert_eq!(
        h.close(&p, &mine, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(h.live_agent_names(T).unwrap(), ["tw_other_web"]);
}

#[test]
fn a_name_already_live_is_refused_including_foreign_agents() {
    let h = FakeHost::new();
    h.add_foreign_agent("tw_demo_api");
    assert_eq!(
        h.spawn(&req("tw_demo_api"), T).unwrap_err().code,
        HostErrorCode::BadArgument
    );
}

#[test]
fn calls_are_recorded_with_their_kind() {
    let h = FakeHost::new();
    let p = h.spawn(&req("tw_demo_api"), T).unwrap();
    h.state(&p, T).unwrap();
    h.read_tail(&p, 5, T).unwrap();
    h.submit(&p, &one_line("hi", 80), T).unwrap();
    h.send_keys(&p, &[Key::Enter], T).unwrap();
    assert_eq!(h.log().count(CallKind::Read), 2);
    assert_eq!(h.log().count(CallKind::Write), 3);
    assert_eq!(
        h.log().names(),
        ["spawn", "state", "read_tail", "submit", "send_keys"]
    );
}

#[test]
fn busy_twice_then_success() {
    let h = FakeHost::new();
    h.inject(
        "spawn",
        Fault::Fail(HostError::new(HostErrorCode::Busy, "busy")),
        2,
    );
    assert_eq!(
        h.spawn(&req("tw_demo_api"), T).unwrap_err().code,
        HostErrorCode::Busy
    );
    assert_eq!(
        h.spawn(&req("tw_demo_api"), T).unwrap_err().code,
        HostErrorCode::Busy
    );
    assert!(h.spawn(&req("tw_demo_api"), T).is_ok());
    assert_eq!(h.log().count_named("spawn"), 3);
}

#[test]
fn every_host_error_code_can_be_injected() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("s", 1));
    for &code in HostErrorCode::ALL {
        h.inject("state", Fault::Fail(HostError::new(code, "x")), 1);
        assert_eq!(h.state(&p, T).unwrap_err().code, code);
    }
    assert!(h.state(&p, T).is_ok());
}

#[test]
fn a_hang_returns_timeout_and_changes_nothing() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("s", 1));
    h.inject("submit", Fault::Hang, 1);
    assert_eq!(
        h.submit(&p, &one_line("x", 9), T).unwrap_err().code,
        HostErrorCode::Timeout
    );
    assert!(h.submitted(&p).is_empty());
}

#[test]
fn accepted_but_reported_failure_still_types_the_text() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("s", 1));
    h.inject(
        "submit",
        Fault::AcceptedButFail(HostError::new(HostErrorCode::Stalled, "no activity")),
        1,
    );
    assert_eq!(
        h.submit(&p, &one_line("hello", 9), T).unwrap_err().code,
        HostErrorCode::Stalled
    );
    assert_eq!(h.submitted(&p), ["hello"]);
}

#[test]
fn wrong_occupant_is_reported_once_and_the_truth_returns() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("mine", 1));
    h.inject("occupant", Fault::WrongOccupant(occ("intruder", 2)), 1);
    assert_eq!(h.occupant(&p, T).unwrap(), occ("intruder", 2));
    assert_eq!(h.occupant(&p, T).unwrap(), occ("mine", 1));
}

#[test]
fn occupant_can_change_between_two_calls() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("mine", 1));
    h.inject(
        "occupant",
        Fault::ChangeOccupantAfter(occ("taken-over", 9)),
        1,
    );
    assert_eq!(h.occupant(&p, T).unwrap(), occ("mine", 1));
    assert_eq!(h.occupant(&p, T).unwrap(), occ("taken-over", 9));
}

#[test]
fn a_host_that_does_not_know_sessions_gives_pids_only() {
    let h = FakeHost::new();
    h.set_caps(HostCaps {
        knows_session_of_place: false,
        ..FakeHost::caps()
    });
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("mine", 1));
    let o = h.occupant(&p, T).unwrap();
    assert!(o.session.is_none());
    assert_eq!(o.pids, [Pid::new(1)]);
}

#[test]
fn a_guarding_host_refuses_to_type_into_a_dialog_and_a_plain_one_types() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Blocked, occ("s", 1));
    assert_eq!(
        h.submit(&p, &one_line("x", 9), T).unwrap_err().code,
        HostErrorCode::Blocked
    );
    h.set_caps(HostCaps {
        guards_dialogs: false,
        ..FakeHost::caps()
    });
    assert!(h.submit(&p, &one_line("x", 9), T).is_ok());
}

#[test]
fn a_host_that_cannot_type_says_unsupported() {
    let h = FakeHost::new();
    h.set_caps(HostCaps {
        can_type: false,
        ..FakeHost::caps()
    });
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("s", 1));
    assert_eq!(
        h.submit(&p, &one_line("x", 9), T).unwrap_err().code,
        HostErrorCode::Unsupported
    );
    assert_eq!(
        h.send_keys(&p, &[Key::Enter], T).unwrap_err().code,
        HostErrorCode::Unsupported
    );
}

#[test]
fn read_tail_returns_the_last_lines() {
    let h = FakeHost::new();
    let p = h.add_place("tw_demo_api", PlaceState::Idle, occ("s", 1));
    h.set_screen(&p, "a\nb\nc\nd");
    assert_eq!(h.read_tail(&p, 2, T).unwrap(), "c\nd");
    assert_eq!(h.read_tail(&p, 10, T).unwrap(), "a\nb\nc\nd");
}

#[test]
fn a_place_from_another_host_is_not_found() {
    let h = FakeHost::new();
    let foreign = Place {
        host: HostId::from_static("other"),
        id: "p1".into(),
    };
    h.add_place("tw_demo_api", PlaceState::Idle, occ("s", 1));
    assert_eq!(
        h.state(&foreign, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
}

#[test]
fn listing_failure_is_injectable_for_many_polls() {
    let h = FakeHost::new();
    h.inject(
        "list",
        Fault::Fail(HostError::new(HostErrorCode::Other, "down")),
        10,
    );
    for _ in 0..10 {
        assert!(h.list(T).is_err());
    }
    assert!(h.list(T).is_ok());
}

#[test]
fn answer_startup_types_into_the_place_only_for_the_plain_dialog() {
    use teamwire_core::{Harness, StartupOutcome, TrustPolicy};
    use teamwire_fakes::{ESCALATED_TRUST_TEXT, FakeHarness, PLAIN_TRUST_TEXT};
    let (h, harness) = (FakeHost::new(), FakeHarness::new());
    let p = h.add_place("tw_demo_api", PlaceState::Blocked, occ("s", 1));
    h.set_screen(&p, PLAIN_TRUST_TEXT);
    let out = harness.answer_startup(&h, &p, TrustPolicy::Accept).unwrap();
    assert_eq!(out, StartupOutcome::AnsweredPlainTrust);
    assert_eq!(h.keys_sent(&p), [Key::Down, Key::Enter]);
    h.set_screen(&p, ESCALATED_TRUST_TEXT);
    let out = harness.answer_startup(&h, &p, TrustPolicy::Accept).unwrap();
    assert_eq!(out, StartupOutcome::RefusedEscalated);
    assert_eq!(h.keys_sent(&p).len(), 2);
    h.set_screen(&p, PLAIN_TRUST_TEXT);
    assert!(harness.answer_startup(&h, &p, TrustPolicy::Refuse).is_err());
    h.set_screen(&p, "> ");
    assert_eq!(
        harness.answer_startup(&h, &p, TrustPolicy::Accept).unwrap(),
        StartupOutcome::NoDialog
    );
}

#[test]
fn a_gone_place_holds_no_live_agent_for_spawn_or_the_name_list() {
    let h = FakeHost::new();
    h.add_place(
        "tw_demo_api",
        PlaceState::Gone,
        Occupant {
            session: None,
            pids: vec![],
        },
    );
    assert!(h.live_agent_names(T).unwrap().is_empty());
    h.spawn(&req("tw_demo_api"), T).unwrap();
    assert_eq!(h.live_agent_names(T).unwrap(), ["tw_demo_api"]);
}
