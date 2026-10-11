#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]

use serde_json::json;
use teamwire_core::platform::ProcessRef;
use teamwire_core::{EventType, HoldInfo, Pid, SessionId, SessionKind, TeamState};

use crate::testkit::{at, count_events, fixture, last_event, new_team};
use crate::{CloseEnd, StoreError};

fn created() -> crate::testkit::Fixture {
    let mut f = fixture();
    f.store
        .write(|tx| tx.create_team(at(0), &new_team()))
        .unwrap();
    f
}

#[test]
fn create_team_writes_the_row_and_its_event() {
    let f = created();
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.state, TeamState::Open);
    assert_eq!(team.title, "Alpha");
    assert_eq!(team.created_at, at(0));
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::TeamCreated);
    assert_eq!(event.subject, Some(team.ulid));
    assert_eq!(event.time, at(0));
    assert_eq!(event.data["lead_route"], "sink");
    assert_eq!(event.data["harness"], "claude-code");
}

#[test]
fn a_second_team_row_is_refused() {
    let mut f = created();
    let again = f.store.write(|tx| tx.create_team(at(1), &new_team()));
    assert!(matches!(again, Err(StoreError::Constraint(_))), "{again:?}");
    assert_eq!(count_events(&f.store, EventType::TeamCreated), 1);
}

#[test]
fn close_lock_is_taken_once_and_its_event_written_once() {
    let mut f = created();
    let took = f
        .store
        .write(|tx| tx.begin_close(at(5), Pid::new(4242)))
        .unwrap();
    assert!(took);
    let second = f
        .store
        .write(|tx| tx.begin_close(at(6), Pid::new(9)))
        .unwrap();
    assert!(!second, "a second closer took the lock");
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.state, TeamState::Closing);
    assert_eq!(team.closing_pid, Some(Pid::new(4242)));
    assert_eq!(count_events(&f.store, EventType::TeamClosing), 1);
    assert_eq!(last_event(&f.store).data["pid"], 4242);
}

#[test]
fn finishing_a_close_completed_or_failed() {
    let mut f = created();
    f.store
        .write(|tx| tx.begin_close(at(1), Pid::new(1)))
        .unwrap();
    let failed = CloseEnd::Failed {
        step: "archive".to_owned(),
    };
    assert!(f.store.write(|tx| tx.finish_close(at(2), &failed)).unwrap());
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!((team.state, team.closing_pid), (TeamState::Open, None));
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::TeamClosed);
    assert_eq!(
        event.data,
        json!({ "outcome": "failed", "failed_step": "archive" })
    );

    f.store
        .write(|tx| tx.begin_close(at(3), Pid::new(1)))
        .unwrap();
    assert!(
        f.store
            .write(|tx| tx.finish_close(at(4), &CloseEnd::Completed))
            .unwrap()
    );
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.state, TeamState::Closed);
    // A closed team cannot be closed again, and finishing needs the lock.
    assert!(
        !f.store
            .write(|tx| tx.begin_close(at(5), Pid::new(1)))
            .unwrap()
    );
    assert!(
        !f.store
            .write(|tx| tx.finish_close(at(6), &CloseEnd::Completed))
            .unwrap()
    );
    assert_eq!(count_events(&f.store, EventType::TeamClosed), 2);
}

#[test]
fn a_stale_close_lock_is_cleared_with_an_event() {
    let mut f = created();
    assert!(
        !f.store
            .write(|tx| tx.clear_stale_close_lock(at(1), None))
            .unwrap()
    );
    f.store
        .write(|tx| tx.begin_close(at(2), Pid::new(77)))
        .unwrap();
    assert!(
        f.store
            .write(|tx| tx.clear_stale_close_lock(at(3), Some(Pid::new(77).into())))
            .unwrap()
    );
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!((team.state, team.closing_pid), (TeamState::Open, None));
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::CloseStep);
    assert_eq!(event.data["step"], "clear_stale_lock");
}

#[test]
fn a_close_lock_held_by_another_pid_is_not_cleared() {
    let mut f = created();
    f.store
        .write(|tx| tx.begin_close(at(2), Pid::new(88)))
        .unwrap();
    // The caller found pid 77 dead, but pid 88 has taken the lock since.
    let cleared = f
        .store
        .write(|tx| tx.clear_stale_close_lock(at(3), Some(Pid::new(77).into())))
        .unwrap();
    assert!(!cleared);
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(
        (team.state, team.closing_pid),
        (TeamState::Closing, Some(Pid::new(88)))
    );
}

#[test]
fn the_close_lock_records_the_owner_start_time_and_drops_it_on_finish() {
    let mut f = created();
    let owner = ProcessRef {
        pid: Pid::new(4242),
        started: Some(at(1)),
    };
    f.store.write(|tx| tx.begin_close(at(5), owner)).unwrap();
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(
        (team.closing_pid, team.closing_started),
        (Some(owner.pid), Some(at(1)))
    );
    f.store
        .write(|tx| tx.finish_close(at(6), &CloseEnd::Failed { step: "x".into() }))
        .unwrap();
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!((team.closing_pid, team.closing_started), (None, None));
    // A bare pid records no start time.
    f.store
        .write(|tx| tx.begin_close(at(7), Pid::new(4242)))
        .unwrap();
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.closing_started, None);
}

#[test]
fn a_close_lock_taken_by_a_reused_pid_is_not_cleared_for_the_dead_owner() {
    let mut f = created();
    // A close by pid 77 that started at 20 holds the lock now. The caller saw
    // pid 77 of an earlier process, started at 1, and found it gone.
    let current = ProcessRef {
        pid: Pid::new(77),
        started: Some(at(20)),
    };
    f.store.write(|tx| tx.begin_close(at(21), current)).unwrap();
    let seen = ProcessRef {
        pid: Pid::new(77),
        started: Some(at(1)),
    };
    let cleared = f
        .store
        .write(|tx| tx.clear_stale_close_lock(at(22), Some(seen)))
        .unwrap();
    assert!(!cleared);
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.state, TeamState::Closing);
    // The owner as it is recorded is cleared, start time and all.
    assert!(
        f.store
            .write(|tx| tx.clear_stale_close_lock(at(23), Some(current)))
            .unwrap()
    );
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(
        (team.state, team.closing_pid, team.closing_started),
        (TeamState::Open, None, None)
    );
}

#[test]
fn adopting_a_lead_records_old_and_new_session() {
    let mut f = created();
    let first = SessionId::parse("sess-1").unwrap();
    let second = SessionId::parse("sess-2").unwrap();
    let old = f
        .store
        .write(|tx| tx.adopt_lead(at(1), &first, SessionKind::Interactive))
        .unwrap();
    assert_eq!(old, None);
    let old = f
        .store
        .write(|tx| tx.adopt_lead(at(2), &second, SessionKind::Background))
        .unwrap();
    assert_eq!(old, Some(first));
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.lead_session, Some(second));
    assert_eq!(team.lead_kind, Some(SessionKind::Background));
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::TeamLeadAdopted);
    assert_eq!(
        event.data,
        json!({ "old_session": "sess-1", "new_session": "sess-2" })
    );
}

#[test]
fn adopting_without_a_team_row_fails() {
    let mut f = fixture();
    let s = SessionId::parse("s").unwrap();
    let r = f
        .store
        .write(|tx| tx.adopt_lead(at(1), &s, SessionKind::Interactive));
    assert!(matches!(r, Err(StoreError::Constraint(_))), "{r:?}");
}

#[test]
fn sleep_hold_is_recorded_and_cleared() {
    let mut f = created();
    let hold = HoldInfo {
        pid: Pid::new(555),
        owned: true,
    };
    f.store.write(|tx| tx.set_sleep_hold(Some(&hold))).unwrap();
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.sleep_hold, Some(hold));
    f.store.write(|tx| tx.set_sleep_hold(None)).unwrap();
    let team = f.store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.sleep_hold, None);
}
