//! A member that becomes quiet after a message to it was stored is never
//! shown that message (MSG-3). A failed member is relaunched in place, and
//! the relaunch may set `quiet` or a quiet kind, so the inbox, the reminder
//! the member hook prints, a resend and status each ask again at read time.
//! The relaunch here makes the writes launch makes for a role that failed
//! before it started: `relaunch_failed_member_with`, then
//! `set_member_marks`, then the session and `started`.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;

use std::time::Duration;

use fixture::{Bed, TEAM, role};
use serde_json::json;
use teamwire_core::{
    Actor, Clock, DeliveryErrorCode, DeliveryState, DisplayId, HarnessId, HostId, MemberKind,
    MemberState, MessageType, Mode, Recipient, SendRequest, TeamName,
};
use teamwire_fakes::{FakeHarness, FakeHost};
use teamwire_store::{Ctx, NewMember, NewMessage};

fn ctx(bed: &mut Bed) -> Ctx<'_> {
    Ctx {
        home: bed.home.path(),
        store: &mut bed.store,
        registry: &bed.registry,
        platform: &bed.platform,
        clock: &bed.clock,
        config: &bed.config,
    }
}

fn new_member(name: &str, kind: MemberKind, bed: &Bed) -> NewMember {
    NewMember {
        role: role(name),
        title: format!("the {name}"),
        kind,
        mode: Mode::Pane,
        harness: HarnessId::from_static(FakeHarness::ID),
        host: HostId::from_static(FakeHost::ID),
        cwd: bed.home.path().to_path_buf(),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: vec![],
        deadline_s: 1800,
    }
}

/// Step 1: a member whose launch failed before it started. It has no
/// session, so a message to it stays `undelivered`.
fn failed(bed: &mut Bed, name: &str, kind: MemberKind) {
    let (now, new) = (bed.clock.now(), new_member(name, kind, bed));
    bed.store
        .write(|tx| {
            tx.insert_member(now, &new)?;
            tx.set_member_state(now, &role(name), MemberState::Failed, &json!({}))
        })
        .unwrap();
}

/// Step 3: the lead launches the failed role again with a new spec.
fn relaunch(bed: &mut Bed, name: &str, kind: MemberKind, quiet: bool) {
    let (now, new) = (bed.clock.now(), new_member(name, kind, bed));
    let r = role(name);
    bed.store
        .write(|tx| {
            assert!(tx.relaunch_failed_member_with(now, &new, &json!({}))?);
            tx.set_member_marks(&r, quiet, None)?;
            tx.set_member_session(&r, &Bed::session(name))?;
            tx.set_member_state(now, &r, MemberState::Started, &json!({}))
        })
        .unwrap();
}

fn send(bed: &mut Bed, from: Actor, to: &str, kind: MessageType, body: &str) -> DisplayId {
    let req = SendRequest {
        team: TeamName::parse(TEAM).unwrap(),
        actor: from,
        to: vec![Recipient::Member(role(to))],
        kind,
        body: body.to_owned(),
        commit: None,
        re: None,
        client_key: None,
    };
    teamwire_msg::send(&mut ctx(bed), req).unwrap().display_id
}

/// What the member is shown: the ids its inbox returns, with and without
/// `--all`, and the reminder its hook prints at a tool call.
fn shown(bed: &mut Bed, name: &str) -> (Vec<String>, Vec<String>, Option<String>) {
    let ids = |items: Vec<teamwire_msg::InboxItem>| {
        items
            .into_iter()
            .map(|i| i.display_id.to_string())
            .collect::<Vec<_>>()
    };
    let reminder = teamwire_report::on_tool_use(&mut ctx(bed), &role(name));
    let unread = ids(teamwire_msg::inbox(&mut ctx(bed), &role(name), false).unwrap());
    let all = ids(teamwire_msg::inbox(&mut ctx(bed), &role(name), true).unwrap());
    (unread, all, reminder)
}

/// Whether status names `id` as stuck, held or unread anywhere.
fn status_names(bed: &mut Bed, id: &DisplayId) -> bool {
    let view = teamwire_watch::status_with(&ctx(bed), None);
    view.stuck_deliveries.iter().any(|d| d.display_id == *id)
        || view.held_deliveries.iter().any(|d| d.display_id == *id)
        || view.unread_deliveries.iter().any(|d| d.display_id == *id)
}

fn delivery(bed: &Bed, id: &DisplayId, to: &str) -> teamwire_core::Delivery {
    bed.store
        .read(|tx| {
            let m = tx.message_by_display_id(id)?.unwrap();
            tx.delivery(&m.ulid, &Recipient::Member(role(to)))
        })
        .unwrap()
        .unwrap()
        .delivery
}

/// The peer's finding of step 2, one the peer typed into the member's
/// session while it was still `creating`, and, stored just after the
/// relaunch as a send that passed its check just before it would be, a
/// third one.
fn peer_messages_then_relaunch(bed: &mut Bed, kind: MemberKind, quiet: bool) -> [DisplayId; 3] {
    bed.add("api");
    failed(bed, "inv", MemberKind::Investigator);
    let before = send(
        bed,
        Actor::Member(role("api")),
        "inv",
        MessageType::Finding,
        "look at x",
    );
    assert_eq!(
        delivery(bed, &before, "inv").state,
        DeliveryState::Undelivered
    );
    let typed = bed.send_from(
        Actor::Member(role("api")),
        MessageType::Finding,
        &Recipient::Member(role("inv")),
        None,
        &"text ".repeat(80),
    );
    let typed = bed
        .store
        .read(|tx| tx.message(&typed))
        .unwrap()
        .unwrap()
        .display_id;
    relaunch(bed, "inv", kind, quiet);
    let now = bed.clock.now();
    let late = bed
        .store
        .write(|tx| {
            tx.insert_message(
                now,
                &NewMessage {
                    sender: Actor::Member(role("api")),
                    kind: MessageType::Finding,
                    re: None,
                    commit: None,
                    body: "look at y".to_owned(),
                    client_key: None,
                    recipients: vec![Recipient::Member(role("inv"))],
                },
            )
        })
        .unwrap()
        .message
        .display_id;
    // The late send's own process is gone; its row is `pending` and stale,
    // which the reminder and a resend treat as left over. The typed one is
    // past the age at which status calls a typed message unread.
    bed.age(Duration::from_secs(700));
    [before, typed, late]
}

fn the_member_sees_nothing_from_the_peer(bed: &mut Bed, peer: &[DisplayId; 3]) {
    let (unread, all, reminder) = shown(bed, "inv");
    let filter = teamwire_msg::LogFilter {
        viewer: Actor::Member(role("inv")),
        role: None,
        limit: None,
    };
    let log: Vec<String> = teamwire_msg::log(&ctx(bed), filter)
        .unwrap()
        .into_iter()
        .map(|r| r.display_id.to_string())
        .collect();
    for id in peer {
        assert!(
            !log.contains(&id.to_string()),
            "the log shows {id}: {log:?}"
        );
    }
    for id in peer {
        let id = id.to_string();
        assert!(!unread.contains(&id), "inbox shows {id}: {unread:?}");
        assert!(!all.contains(&id), "inbox --all shows {id}: {all:?}");
        assert!(
            !reminder.as_deref().unwrap_or("").contains(&id),
            "the reminder names {id}: {reminder:?}"
        );
    }
    for id in [&peer[0], &peer[2]] {
        let d = delivery(bed, id, "inv");
        assert_eq!(d.state, DeliveryState::Undelivered, "{id}");
        assert_eq!(
            d.error.map(|e| e.code),
            Some(DeliveryErrorCode::RecipientNotAllowed),
            "{id}"
        );
    }
    // A typed row may be on the old screen, so it cannot move back.
    assert_eq!(
        delivery(bed, &peer[1], "inv").state,
        DeliveryState::HandedOff
    );
    for id in peer {
        assert!(!status_names(bed, id), "status lists {id}");
    }
    // Marking one read would tell the peer's log that the member read it.
    let acked = teamwire_msg::ack(&mut ctx(bed), &role("inv"), peer).unwrap();
    assert!(
        acked
            .iter()
            .all(|a| a.outcome == teamwire_msg::AckOutcome::Unknown),
        "{acked:?}"
    );
}

#[test]
fn a_member_relaunched_with_quiet_sees_nothing_a_peer_sent_earlier() {
    let mut bed = Bed::new();
    let peer = peer_messages_then_relaunch(&mut bed, MemberKind::Investigator, true);
    // A resend of the late message reaches the typing step, which types
    // nothing to a member that may not be shown it.
    let resent = teamwire_msg::resend(
        &mut ctx(&mut bed),
        &Actor::Member(role("api")),
        &peer[2],
        None,
    )
    .unwrap();
    assert_eq!(
        resent.deliveries.first().and_then(|d| d.reason),
        Some(DeliveryErrorCode::RecipientNotAllowed),
        "{resent:?}"
    );
    the_member_sees_nothing_from_the_peer(&mut bed, &peer);
    let again = teamwire_msg::resend(
        &mut ctx(&mut bed),
        &Actor::Member(role("api")),
        &peer[0],
        None,
    );
    assert!(
        matches!(&again, Err(teamwire_msg::SendError::NotResendable(why)) if why.contains("quiet")),
        "{again:?}"
    );
}

#[test]
fn a_member_relaunched_as_a_reviewer_sees_nothing_a_peer_sent_earlier() {
    let mut bed = Bed::new();
    let peer = peer_messages_then_relaunch(&mut bed, MemberKind::Reviewer, false);
    the_member_sees_nothing_from_the_peer(&mut bed, &peer);
}

#[test]
fn a_member_relaunched_with_quiet_still_gets_its_own_leads_messages() {
    let mut bed = Bed::new();
    failed(&mut bed, "inv", MemberKind::Investigator);
    let before = send(
        &mut bed,
        Actor::Lead,
        "inv",
        MessageType::Note,
        "read the brief again",
    );
    relaunch(&mut bed, "inv", MemberKind::Investigator, true);
    let after = send(
        &mut bed,
        Actor::Lead,
        "inv",
        MessageType::Note,
        "and the plan",
    );
    let (unread, _, reminder) = shown(&mut bed, "inv");
    let reminder = reminder.unwrap_or_default();
    for id in [&before, &after] {
        let id = id.to_string();
        assert!(unread.contains(&id), "inbox misses {id}: {unread:?}");
        assert!(
            reminder.contains(&id),
            "the reminder misses {id}: {reminder}"
        );
    }
    assert_eq!(delivery(&bed, &before, "inv").state, DeliveryState::Acked);
}
