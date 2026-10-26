//! When a `pending` delivery counts as left behind: each recipient keeps
//! its own clock, and a row for an agent of another team is never mistaken
//! for the lead's, though neither has a role.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use teamwire_core::{
    Actor, HarnessId, HostId, LeadRoute, Message, MessageType, Recipient, Role, TeamName, Timestamp,
};
use teamwire_deliver::{PENDING_STALE, attempt_started, is_pending_stale, stops_since_attempt};
use teamwire_fakes::TestHome;
use teamwire_store::{NewMessage, NewTeam, Store};

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

/// A team with one message from the lead to `recipients`, sent at `at(1)`.
fn one_message(store: &mut Store, team: &TeamName, recipients: Vec<Recipient>) -> Message {
    store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: team.clone(),
                    title: "Alpha".to_owned(),
                    lead_harness: HarnessId::from_static("fake"),
                    lead_host: HostId::from_static("fake"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            tx.insert_message(
                at(1),
                &NewMessage {
                    sender: Actor::Lead,
                    kind: MessageType::Finding,
                    re: None,
                    commit: None,
                    body: "x".to_owned(),
                    client_key: None,
                    recipients,
                },
            )
        })
        .unwrap()
        .message
}

#[test]
fn a_resend_to_another_team_restarts_only_that_rows_clock() {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("alpha").unwrap();
    let mut store = Store::open(home.path(), &team).unwrap();
    let other = Recipient::parse("db@beta").unwrap();
    let message = one_message(&mut store, &team, vec![Recipient::Lead, other.clone()]);
    store
        .write(|tx| tx.begin_resend(at(100), &message.ulid, &other))
        .unwrap();
    let (lead, remote) = store
        .read(|tx| {
            Ok((
                attempt_started(tx, &message, &Recipient::Lead)?,
                attempt_started(tx, &message, &other)?,
            ))
        })
        .unwrap();
    assert_eq!(lead, at(1));
    assert_eq!(remote, at(100));
    // One second before the resent row would go stale; the lead's row is long stale.
    let now = at(100 + i64::try_from(PENDING_STALE.as_secs()).unwrap() - 1);
    let (lead_stale, remote_stale) = store
        .read(|tx| {
            Ok((
                is_pending_stale(tx, &message, &Recipient::Lead, now)?,
                is_pending_stale(tx, &message, &other, now)?,
            ))
        })
        .unwrap();
    assert!(lead_stale);
    assert!(!remote_stale);
}

fn member(role: &str) -> Recipient {
    Recipient::Member(Role::parse(role).unwrap())
}

#[test]
fn a_stop_of_the_recipient_during_the_attempt_is_counted_and_a_resend_starts_again() {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("alpha").unwrap();
    let mut store = Store::open(home.path(), &team).unwrap();
    let (inv, a) = (member("inv"), member("a"));
    let message = one_message(&mut store, &team, vec![inv.clone(), a.clone()]);
    let stops = |store: &Store, to: &Recipient| {
        store
            .read(|tx| stops_since_attempt(tx, &message, to))
            .unwrap()
    };
    assert_eq!(stops(&store, &inv), 0);

    store
        .write(|tx| tx.record_recipient_stopped(at(2), &message.ulid, &inv))
        .unwrap();
    assert_eq!(stops(&store, &inv), 1);
    // The other recipient's row has its own attempt and its own stops.
    assert_eq!(stops(&store, &a), 0);

    store
        .write(|tx| tx.begin_resend(at(3), &message.ulid, &inv))
        .unwrap();
    assert_eq!(
        stops(&store, &inv),
        0,
        "a stop before the resend is of the earlier attempt"
    );
    store
        .write(|tx| tx.record_recipient_stopped(at(4), &message.ulid, &inv))
        .unwrap();
    assert_eq!(stops(&store, &inv), 1);
}
