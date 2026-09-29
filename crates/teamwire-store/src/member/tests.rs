#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]

use serde_json::json;
use teamwire_core::{EventType, MemberState, Place, SessionId};

use crate::StoreError;
use crate::testkit::{at, count_events, event_types, last_event, new_member, role, with_members};

fn state_of(f: &crate::testkit::Fixture, name: &str) -> MemberState {
    f.store
        .read(|tx| tx.member(&role(name)))
        .unwrap()
        .unwrap()
        .state
}

#[test]
fn insert_member_starts_creating_at_generation_one_with_its_event() {
    let f = with_members(&["api"]);
    let m = f.store.read(|tx| tx.member(&role("api"))).unwrap().unwrap();
    assert_eq!((m.state, m.generation), (MemberState::Creating, 1));
    assert_eq!(m.gen_started_at, at(1));
    assert_eq!(m.owns, ["src/"]);
    assert_eq!(m.check.as_deref(), Some("cargo test"));
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::MemberCreating);
    assert_eq!(event.role, Some(role("api")));
    assert_eq!(event.data["kind"], "implementer");
}

#[test]
fn the_same_role_cannot_be_inserted_twice() {
    let mut f = with_members(&["api"]);
    let again = f
        .store
        .write(|tx| tx.insert_member(at(2), &new_member("api")));
    assert!(matches!(again, Err(StoreError::Constraint(_))), "{again:?}");
    assert_eq!(count_events(&f.store, EventType::MemberCreating), 1);
}

#[test]
fn place_session_and_token_are_recorded_and_the_hash_stays_out_of_member() {
    let mut f = with_members(&["api"]);
    let events_before = event_types(&f.store).len();
    let r = role("api");
    let place = Place {
        host: teamwire_core::HostId::from_static("herdr"),
        id: "w3T:p3".to_owned(),
    };
    let session = SessionId::parse("sess-a").unwrap();
    f.store
        .write(|tx| {
            tx.set_member_place(&r, &place)?;
            tx.set_member_session(&r, &session)?;
            tx.set_member_token_hash(&r, &[7; 32])
        })
        .unwrap();
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!(m.place, Some(place));
    assert_eq!(m.session_id, Some(session));
    assert_eq!(
        f.store.read(|tx| tx.member_token_hash(&r)).unwrap(),
        Some([7; 32])
    );
    assert_eq!(
        f.store
            .read(|tx| tx.member_token_hash(&role("none")))
            .unwrap(),
        None
    );
    // These three carry no event of their own.
    assert_eq!(event_types(&f.store).len(), events_before);
    let missing = f
        .store
        .write(|tx| tx.set_member_token_hash(&role("ghost"), &[0; 32]));
    assert!(matches!(missing, Err(StoreError::Constraint(_))));
}

#[test]
fn state_moves_write_their_event_and_only_go_forward() {
    let mut f = with_members(&["api", "web"]);
    let r = role("api");
    f.store
        .write(|tx| {
            tx.set_member_state(at(2), &r, MemberState::Started, &json!({ "seconds": 1.5 }))
        })
        .unwrap();
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::MemberStarted);
    assert_eq!(event.data["seconds"], 1.5);
    assert_eq!(state_of(&f, "api"), MemberState::Started);

    for bad in [
        MemberState::Creating,
        MemberState::Started,
        MemberState::Removed,
    ] {
        let r2 = f
            .store
            .write(|tx| tx.set_member_state(at(3), &r, bad, &json!({})));
        assert!(
            matches!(r2, Err(StoreError::Constraint(_))),
            "{bad}: {r2:?}"
        );
    }
    assert_eq!(state_of(&f, "api"), MemberState::Started);

    let w = role("web");
    f.store
        .write(|tx| {
            tx.set_member_state(
                at(4),
                &w,
                MemberState::Failed,
                &json!({ "reason": "trust" }),
            )
        })
        .unwrap();
    assert_eq!(last_event(&f.store).kind, EventType::MemberFailed);
    let back = f
        .store
        .write(|tx| tx.set_member_state(at(5), &w, MemberState::Started, &json!({})));
    assert!(matches!(back, Err(StoreError::Constraint(_))));
    let ghost = f
        .store
        .write(|tx| tx.set_member_state(at(5), &role("ghost"), MemberState::Started, &json!({})));
    assert!(matches!(ghost, Err(StoreError::Constraint(_))));
}

#[test]
fn removal_keeps_the_row_and_is_final() {
    let mut f = with_members(&["api"]);
    let r = role("api");
    assert!(
        f.store
            .write(|tx| tx.remove_member(at(9), &r, Some("done"), &json!({ "unverified": false })))
            .unwrap()
    );
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!(m.state, MemberState::Removed);
    assert_eq!(m.removed_at, Some(at(9)));
    assert_eq!(m.removed_note.as_deref(), Some("done"));
    assert_eq!(last_event(&f.store).kind, EventType::MemberRemoved);
    let events = count_events(&f.store, EventType::MemberRemoved);
    assert!(
        !f.store
            .write(|tx| tx.remove_member(at(10), &r, None, &json!({})))
            .unwrap()
    );
    assert_eq!(count_events(&f.store, EventType::MemberRemoved), events);
    let revive = f
        .store
        .write(|tx| tx.set_member_state(at(11), &r, MemberState::Started, &json!({})));
    assert!(revive.is_err());
    let ghost = f
        .store
        .write(|tx| tx.remove_member(at(12), &role("ghost"), None, &json!({})));
    assert!(matches!(ghost, Err(StoreError::Constraint(_))));
}

#[test]
fn relaunching_a_failed_member_writes_replaced_and_creating() {
    let mut f = with_members(&["api"]);
    let r = role("api");
    // Only a failed row may be relaunched.
    assert!(
        !f.store
            .write(|tx| tx.relaunch_failed_member(at(2), &new_member("api")))
            .unwrap()
    );
    assert!(
        !f.store
            .write(|tx| tx.relaunch_failed_member(at(2), &new_member("ghost")))
            .unwrap()
    );
    f.store
        .write(|tx| {
            tx.set_member_session(&r, &SessionId::parse("old").unwrap())?;
            tx.set_member_state(at(3), &r, MemberState::Failed, &json!({}))
        })
        .unwrap();
    let mut again = new_member("api");
    again.title = "second try".to_owned();
    assert!(
        f.store
            .write(|tx| tx.relaunch_failed_member(at(60), &again))
            .unwrap()
    );
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!(
        (m.state, m.title.as_str(), m.session_id),
        (MemberState::Creating, "second try", None)
    );
    assert_eq!(m.gen_started_at, at(60));
    let tail = event_types(&f.store);
    assert_eq!(
        tail[tail.len() - 2..],
        ["member.replaced", "member.creating"]
    );
}

#[test]
fn bumping_the_generation_and_undoing_it() {
    let mut f = with_members(&["api"]);
    let r = role("api");
    let g = f
        .store
        .write(|tx| tx.bump_generation(at(30), &r, "please fix the tests"))
        .unwrap();
    assert_eq!(g, 2);
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!((m.generation, m.gen_started_at), (2, at(30)));
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::MemberReprompted);
    assert_eq!(
        event.data,
        json!({ "generation": 2, "message": "please fix the tests" })
    );

    f.store
        .write(|tx| tx.restore_generation(at(31), &r, 1, at(1)))
        .unwrap();
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!((m.generation, m.gen_started_at), (1, at(1)));
    assert_eq!(last_event(&f.store).data["undone"], true);
    // It cannot go back twice.
    let twice = f
        .store
        .write(|tx| tx.restore_generation(at(32), &r, 1, at(1)));
    assert!(matches!(twice, Err(StoreError::Constraint(_))));
}

#[test]
fn a_removed_or_failed_member_cannot_be_reprompted() {
    let mut f = with_members(&["api", "web"]);
    f.store
        .write(|tx| tx.set_member_state(at(2), &role("web"), MemberState::Failed, &json!({})))
        .unwrap();
    f.store
        .write(|tx| tx.remove_member(at(3), &role("api"), None, &json!({})))
        .unwrap();
    for name in ["api", "web"] {
        let r = f
            .store
            .write(|tx| tx.bump_generation(at(4), &role(name), "x"));
        assert!(matches!(r, Err(StoreError::Constraint(_))), "{name}: {r:?}");
    }
    assert_eq!(count_events(&f.store, EventType::MemberReprompted), 0);
}

#[test]
fn holding_a_member_records_the_generation_and_writes_member_held() {
    let mut f = with_members(&["api"]);
    let r = role("api");
    f.store
        .write(|tx| {
            tx.hold_member(
                at(5),
                &r,
                1,
                &json!({ "gaps": ["report_missing"], "unread": 0 }),
            )
        })
        .unwrap();
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!(m.gate_told_generation, Some(1));
    assert_eq!(last_event(&f.store).kind, EventType::MemberHeld);
    f.store
        .write(|tx| tx.set_gate_told_generation(&r, 2))
        .unwrap();
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!(m.gate_told_generation, Some(2));
}

#[test]
fn members_come_back_in_insert_order_including_removed() {
    let mut f = with_members(&["zed", "api", "mid"]);
    f.store
        .write(|tx| tx.remove_member(at(2), &role("api"), None, &json!({})))
        .unwrap();
    let listing = f.store.read(|tx| tx.members()).unwrap();
    let roles: Vec<&str> = listing.items.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["zed", "api", "mid"]);
    assert!(listing.bad.is_empty());
}

#[test]
fn an_unread_hold_leaves_the_report_gap_hold_unused() {
    let mut f = with_members(&["api"]);
    let r = role("api");
    f.store
        .write(|tx| tx.hold_member_unread(at(5), &r, &json!({ "unread": 2 })))
        .unwrap();
    let m = f.store.read(|tx| tx.member(&r)).unwrap().unwrap();
    assert_eq!(m.gate_told_generation, None);
    let e = last_event(&f.store);
    assert_eq!(
        (e.kind, e.data["unread"].clone()),
        (EventType::MemberHeld, json!(2))
    );
    let ghost = f
        .store
        .write(|tx| tx.hold_member_unread(at(6), &role("ghost"), &json!({})));
    assert!(matches!(ghost, Err(StoreError::Constraint(_))));
}

#[test]
fn a_role_that_reached_started_is_not_relaunched() {
    let mut f = with_members(&["api"]);
    let r = role("api");
    f.store
        .write(|tx| {
            tx.set_member_state(at(2), &r, MemberState::Started, &json!({}))?;
            tx.set_member_state(at(3), &r, MemberState::Failed, &json!({}))
        })
        .unwrap();
    let refused = f
        .store
        .write(|tx| tx.relaunch_failed_member(at(4), &new_member("api")));
    assert!(
        matches!(refused, Err(StoreError::Constraint(_))),
        "{refused:?}"
    );
    assert_eq!(state_of(&f, "api"), MemberState::Failed);
    assert_eq!(count_events(&f.store, EventType::MemberReplaced), 0);
}

#[test]
fn creating_event_carries_extra_data_but_kind_and_mode_win() {
    let mut f = with_members(&[]);
    let extra = json!({ "launch": "01L", "requested": 3, "kind": "forged" });
    f.store
        .write(|tx| tx.insert_member_with(at(1), &new_member("api"), &extra))
        .unwrap();
    let event = last_event(&f.store);
    assert_eq!(event.kind, EventType::MemberCreating);
    assert_eq!(
        (
            &event.data["launch"],
            &event.data["requested"],
            &event.data["kind"]
        ),
        (&json!("01L"), &json!(3), &json!("implementer"))
    );
    let r = role("api");
    f.store
        .write(|tx| tx.set_member_state(at(2), &r, MemberState::Failed, &json!({})))
        .unwrap();
    let again = json!({ "launch": "02L", "requested": 1 });
    assert!(
        f.store
            .write(|tx| tx.relaunch_failed_member_with(at(3), &new_member("api"), &again))
            .unwrap()
    );
    assert_eq!(last_event(&f.store).data["launch"], "02L");
}
