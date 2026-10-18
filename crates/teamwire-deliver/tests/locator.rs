//! The locator refuses every case ADR 0006 lists, and the property that it
//! answers only when the two views name exactly one common place.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::panic,
    reason = "a helper that panics on the wrong outcome fails the test, which is the intended result"
)]

#[path = "support/members.rs"]
mod members;
#[path = "support/rig.rs"]
mod rig;

use proptest::prelude::*;
use teamwire_core::{
    DeliveryErrorCode, HarnessError, HarnessErrorCode, HostError, HostErrorCode, Liveness,
    Occupant, Pid, PlaceState, SessionKind, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line, locate, locate_session};
use teamwire_fakes::{CallKind, Fault};
use teamwire_store::Party;

use rig::{Rig, sid};

fn code_of(outcome: DeliveryOutcome) -> DeliveryErrorCode {
    match outcome {
        DeliveryOutcome::Undelivered(e) | DeliveryOutcome::Unconfirmed(e) => e.code,
        DeliveryOutcome::HandedOff => panic!("expected a refusal, got a hand-off"),
    }
}

fn refusal(rig: &mut Rig, party: &Party) -> DeliveryErrorCode {
    locate(&rig.ctx(), party).unwrap_err().code
}

fn holds(session: &str, pids: &[u32]) -> Occupant {
    Occupant {
        session: Some(sid(session)),
        pids: pids.iter().copied().map(Pid::new).collect(),
    }
}

#[test]
fn a_session_no_store_records_is_located_by_the_same_two_views() {
    let rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    let found = |session: &str| {
        locate_session(
            &rig.platform,
            rig.clock.as_ref(),
            &sid(session),
            rig.harness.as_ref(),
            rig.host.as_ref(),
        )
    };
    assert_eq!(found("lead-1").unwrap(), place);
    assert_eq!(
        found("nobody").unwrap_err().code,
        DeliveryErrorCode::NoLiveProcess
    );
    // The host view still has to agree: a pane another session took refuses.
    rig.host.set_occupant(&place, holds("intruder", &[]));
    assert_eq!(
        found("lead-1").unwrap_err().code,
        DeliveryErrorCode::HostOtherSession
    );
}

#[test]
fn takeover_session_in_lead_place_gets_nothing() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    // Another conversation now holds the lead's pane (INV-1).
    rig.host.set_occupant(&place, holds("intruder", &[]));
    let line = one_line("hello", 80);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &line);
    assert_eq!(code_of(outcome), DeliveryErrorCode::HostOtherSession);
    assert!(rig.host.submitted(&place).is_empty());
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn host_listing_failure_refuses() {
    for fault in [
        Fault::Fail(HostError::new(HostErrorCode::Other, "herdr exited 1")),
        Fault::Hang,
    ] {
        let mut rig = Rig::new();
        let place = rig.healthy("lead-1", 100);
        rig.host.inject_always("occupant", fault);
        let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
        assert_eq!(code_of(outcome), DeliveryErrorCode::HostListingFailed);
        assert!(rig.host.submitted(&place).is_empty());
    }
}

#[test]
fn a_host_listing_failure_keeps_a_short_one_line_host_error() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    let long = format!(
        "tmux: ps failed\n\u{1b}[31mexit 1\u{1b}[0m\n{}",
        "x".repeat(500)
    );
    rig.host.inject_always(
        "occupant",
        Fault::Fail(HostError::new(HostErrorCode::Other, long)),
    );
    let DeliveryOutcome::Undelivered(e) =
        deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80))
    else {
        unreachable!("the listing failed, so nothing was delivered");
    };
    assert_eq!(e.code, DeliveryErrorCode::HostListingFailed);
    assert!(e.detail.contains("tmux: ps failed exit 1"), "{}", e.detail);
    assert!(!e.detail.contains('\n') && !e.detail.contains('\u{1b}'));
    assert!(e.detail.chars().count() <= 200, "{}", e.detail.len());
}

#[test]
fn session_in_two_places_refuses() {
    let mut rig = Rig::new();
    let a = rig.healthy("lead-1", 100);
    let b = rig
        .host
        .add_place("second", PlaceState::Idle, holds("lead-1", &[]));
    rig.harness.add_session(
        sid("lead-1"),
        vec![Pid::new(100), Pid::new(101)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.claim(101, &b.id);
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::TwoPlaces
    );
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(code_of(outcome), DeliveryErrorCode::TwoPlaces);
    assert!(rig.host.submitted(&a).is_empty() && rig.host.submitted(&b).is_empty());
}

#[test]
fn place_changing_hands_during_the_dialog_check_is_refused() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    rig.host.inject(
        "occupant",
        Fault::ChangeOccupantAfter(holds("intruder", &[])),
        1,
    );
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(code_of(outcome), DeliveryErrorCode::HostOtherSession);
    assert!(rig.host.submitted(&place).is_empty());
    // The occupant check is the last read before the submit, never the
    // state or screen reads that precede it.
    let names = rig.host.log().names();
    let last_occupant = names.iter().rposition(|n| *n == "occupant").unwrap();
    let last_other_read = names
        .iter()
        .rposition(|n| matches!(*n, "state" | "read_tail"))
        .unwrap();
    assert!(last_other_read < last_occupant, "{names:?}");
}

#[test]
fn place_changing_hands_between_retries_is_refused() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    // The first fault changes nothing and uses up the locate call; the
    // second lets the first recheck pass and then hands the place over.
    rig.host.inject(
        "occupant",
        Fault::ChangeOccupantAfter(holds("lead-1", &[])),
        1,
    );
    rig.host.inject(
        "occupant",
        Fault::ChangeOccupantAfter(holds("intruder", &[])),
        1,
    );
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Busy, "scripted")),
        1,
    );
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(code_of(outcome), DeliveryErrorCode::HostOtherSession);
    assert_eq!(rig.host.log().count_named("submit"), 1);
    assert!(rig.host.submitted(&place).is_empty());
}

#[test]
fn occupant_changes_before_submit_refuses() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    // The first occupant call tells the truth; the place changes hands after.
    rig.host.inject(
        "occupant",
        Fault::ChangeOccupantAfter(holds("intruder", &[])),
        1,
    );
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(code_of(outcome), DeliveryErrorCode::HostOtherSession);
    assert_eq!(rig.host.log().count_named("occupant"), 2);
    assert!(rig.host.submitted(&place).is_empty());
    assert_eq!(rig.host.log().count_named("submit"), 0);
}

#[test]
fn resumed_lead_in_new_place_receives() {
    let mut rig = Rig::new();
    let old = rig.healthy("lead-1", 100);
    // `claude --resume`: same session id, new process, new place (INV-2).
    rig.host.set_occupant(&old, holds("someone-else", &[]));
    rig.platform.set_alive(Pid::new(100), false);
    let new = rig.healthy("lead-1", 200);
    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), new);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&new), ["hello"]);
    assert!(rig.host.submitted(&old).is_empty());
}

#[test]
fn stale_claimant_ignored_when_host_confirms_one() {
    let mut rig = Rig::new();
    let stale = rig
        .host
        .add_place("stale", PlaceState::Idle, holds("someone-else", &[]));
    let live = rig.healthy("lead-1", 100);
    rig.harness.add_session(
        sid("lead-1"),
        vec![Pid::new(99), Pid::new(100)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.claim(99, &stale.id);
    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), live);

    // A claimant whose place the host no longer lists is ignored the same way.
    rig.claim(99, "gone-pane");
    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), live);
}

#[test]
fn no_live_process_refuses() {
    let mut rig = Rig::new();
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::NoLiveProcess
    );
    rig.harness
        .fail_listing(Some(HarnessError::new(HarnessErrorCode::Timeout, "slow")));
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::NoLiveProcess
    );
}

#[test]
fn process_without_place_variable_refuses() {
    let mut rig = Rig::new();
    rig.harness.add_session(
        sid("lead-1"),
        vec![Pid::new(100)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.platform.set_alive(Pid::new(100), true);
    assert_eq!(refusal(&mut rig, &Party::Lead), DeliveryErrorCode::NoPlace);
}

#[test]
fn empty_place_refuses_as_no_session() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![],
        },
    );
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::HostNoSession
    );
}

#[test]
fn one_view_only_refuses_as_single_source() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    rig.host.set_place_env_var(None);
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::SingleSource
    );
}

#[test]
fn background_lead_refuses() {
    let mut rig = Rig::with_lead_kind(SessionKind::Background);
    rig.healthy("lead-1", 100);
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::BackgroundSession
    );
}

#[test]
fn host_that_knows_only_pids_confirms_by_pid() {
    let mut rig = Rig::new();
    let place = rig.healthy("lead-1", 100);
    // tmux-style host: no session id, the pids in the pane's process tree.
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![Pid::new(100), Pid::new(101)],
        },
    );
    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), place);
    rig.host.set_occupant(
        &place,
        Occupant {
            session: None,
            pids: vec![Pid::new(555)],
        },
    );
    assert_eq!(
        refusal(&mut rig, &Party::Lead),
        DeliveryErrorCode::HostOtherSession
    );
}

#[test]
fn member_is_located_by_its_own_session() {
    let mut rig = Rig::new();
    let member = rig.add_member("api", "fake", "member-1");
    let lead_place = rig.healthy("lead-1", 100);
    let member_place = rig.healthy("member-1", 200);
    assert_eq!(locate(&rig.ctx(), &member).unwrap(), member_place);
    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), lead_place);
}

#[test]
fn lookup_holds_no_write_transaction_and_makes_only_read_calls() {
    let mut rig = Rig::new();
    rig.healthy("lead-1", 100);
    rig.host.log().clear();
    locate(&rig.ctx(), &Party::Lead).unwrap();
    assert_eq!(rig.host.log().count(CallKind::Write), 0);
    // A second connection can take the write lock right away.
    let mut other = teamwire_store::Store::open_existing(
        rig.home.path(),
        &teamwire_core::TeamName::parse("alpha").unwrap(),
    )
    .unwrap();
    other.write(|_| Ok(())).unwrap();
}

#[derive(Debug, Clone, Copy)]
enum Held {
    Ours,
    Other,
    Empty,
}

fn held() -> impl Strategy<Value = Held> {
    prop_oneof![Just(Held::Ours), Just(Held::Other), Just(Held::Empty)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// A place comes back only when exactly one place is both named by a
    /// live process of the session and confirmed by the host.
    #[test]
    fn locator_answers_only_for_one_common_place(
        places in prop::collection::vec((held(), any::<bool>()), 0..5),
        ghosts in 0usize..3,
    ) {
        let mut rig = Rig::new();
        let mut pids = Vec::new();
        let mut confirmed = Vec::new();
        let mut next_pid = 100_u32;
        for (i, (who, claimed)) in places.iter().enumerate() {
            let occupant = match who {
                Held::Ours => holds("lead-1", &[]),
                Held::Other => holds("other", &[]),
                Held::Empty => Occupant { session: None, pids: vec![] },
            };
            let place = rig.host.add_place(&format!("p{i}"), PlaceState::Idle, occupant);
            if *claimed {
                rig.claim(next_pid, &place.id);
                pids.push(Pid::new(next_pid));
                next_pid += 1;
                if matches!(who, Held::Ours) {
                    confirmed.push(place);
                }
            }
        }
        for g in 0..ghosts {
            rig.claim(next_pid, &format!("ghost{g}"));
            pids.push(Pid::new(next_pid));
            next_pid += 1;
        }
        rig.harness.add_session(sid("lead-1"), pids, Liveness::Alive(PlaceState::Idle));
        let result = locate(&rig.ctx(), &Party::Lead);
        match confirmed.len() {
            1 => prop_assert_eq!(result.unwrap(), confirmed.remove(0)),
            n if n > 1 => prop_assert_eq!(result.unwrap_err().code, DeliveryErrorCode::TwoPlaces),
            _ => prop_assert!(result.is_err()),
        }
    }
}

#[test]
fn a_host_that_joins_two_variables_gets_the_joined_place() {
    let mut rig = Rig::new();
    rig.host.set_place_suffix_var(Some("FAKE_SOCKET"));
    let place = rig.host.add_place_with_id(
        "pane-100@/tmp/sock".to_owned(),
        "pane-100",
        PlaceState::Idle,
        holds("lead-1", &[]),
    );
    rig.harness.add_session(
        sid("lead-1"),
        vec![Pid::new(100)],
        Liveness::Alive(PlaceState::Idle),
    );
    rig.claim(100, "pane-100");
    rig.platform
        .set_env(Pid::new(100), "FAKE_SOCKET", "/tmp/sock");
    assert_eq!(locate(&rig.ctx(), &Party::Lead).unwrap(), place);
}
