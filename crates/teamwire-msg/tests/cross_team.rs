//! Messages between teams (MSG-20, ADR 0016): the two copies under one
//! ULID, the line typed in the other team's pane, answers in both
//! directions, the refusals, and a resend that repairs a send that died.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test indexes what it built itself; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;
#[path = "support/teams.rs"]
mod teams;

use std::time::Duration;

use teamwire_core::{
    Actor, DeliveryErrorCode, DeliveryState, EventType, MemberKind, MessageType, Place, Recipient,
    Refusal, Remote, SendRequest, TeamName, Ulid,
};
use teamwire_msg::{LogFilter, RecipientState, SendError, log, resend, send};
use teamwire_store::{MessageFilter, NewMessage, ReceivedMessage, Store};

use rig::{Rig, id, role, to};

struct Two {
    rig: Rig,
    beta: Store,
    /// The places of beta's lead and members, by name.
    places: Vec<(String, Place)>,
    api: Place,
}

/// Team alpha with `api` and a reviewer; team beta with an implementer
/// `db`, an investigator `scout` and a reviewer `rev`.
fn two_teams() -> Two {
    let mut rig = Rig::new();
    let api = rig.add_member("api", MemberKind::Implementer);
    rig.add_member("arev", MemberKind::Reviewer);
    let places = rig.add_team(
        "beta",
        "lead-2",
        &[
            ("db", MemberKind::Implementer),
            ("scout", MemberKind::Investigator),
            ("rev", MemberKind::Reviewer),
        ],
    );
    let beta = rig.store_of("beta");
    Two {
        rig,
        beta,
        places,
        api,
    }
}

impl Two {
    fn place(&self, name: &str) -> &Place {
        &self.places.iter().find(|(n, _)| n == name).unwrap().1
    }

    fn typed(&self, name: &str) -> Vec<String> {
        self.rig.host.submitted(self.place(name))
    }

    /// A send from beta's own party `from`.
    fn send_from_beta(&mut self, req: SendRequest) -> Result<teamwire_msg::SendReport, SendError> {
        send(&mut self.rig.ctx_on(&mut self.beta), req)
    }
}

fn beta_request(from: &str, targets: &[&str], kind: MessageType, text: &str) -> SendRequest {
    SendRequest {
        team: TeamName::parse("beta").unwrap(),
        actor: Actor::parse(from).unwrap(),
        to: targets.iter().map(|t| to(t)).collect(),
        kind,
        body: text.to_owned(),
        commit: None,
        re: None,
        client_key: None,
    }
}

fn refused(result: Result<teamwire_msg::SendReport, SendError>) -> Option<Refusal> {
    match result {
        Err(SendError::Refused(r)) => Some(r),
        Ok(_)
        | Err(
            SendError::NotResendable(_)
            | SendError::InvalidArgument(_)
            | SendError::Store(_)
            | SendError::Internal(_),
        ) => None,
    }
}

fn messages(store: &Store) -> Vec<teamwire_core::Message> {
    store
        .read(|tx| tx.messages(&MessageFilter::default())?.all_or_corrupt())
        .unwrap()
}

fn state_in(store: &Store, ulid: &Ulid, recipient: &str) -> DeliveryState {
    store
        .read(|tx| tx.delivery(ulid, &to(recipient)))
        .unwrap()
        .unwrap()
        .delivery
        .state
}

#[test]
fn a_member_reaches_a_member_of_another_team() {
    let mut t = two_teams();
    let req = t.rig.request(
        "api",
        &["db@beta"],
        MessageType::Finding,
        "the users table moved to schema v2",
    );
    let report = send(&mut t.rig.ctx(), req).unwrap();
    let only = &report.deliveries[0];
    assert_eq!(only.to.as_str(), "db@beta");
    assert_eq!(only.state, RecipientState::HandedOff);

    // One message, one ULID, in both teams.
    let here = messages(&t.rig.store);
    let there = messages(&t.beta);
    assert_eq!((here.len(), there.len()), (1, 1));
    assert_eq!(here[0].ulid, there[0].ulid);
    assert_eq!(there[0].sender.as_str(), "api@alpha");
    assert_eq!(there[0].body, here[0].body);
    assert_eq!(
        state_in(&t.beta, &there[0].ulid, "db"),
        DeliveryState::HandedOff
    );
    assert_eq!(
        state_in(&t.rig.store, &here[0].ulid, "db@beta"),
        DeliveryState::HandedOff
    );

    let typed = t.typed("db");
    assert_eq!(typed.len(), 1);
    assert!(
        typed[0].starts_with("[teamwire:beta msg m001 finding from api@alpha]"),
        "{}",
        typed[0]
    );
    assert!(
        typed[0].contains("from team alpha: information to check"),
        "{}",
        typed[0]
    );
    // The comms copy is written in beta's own directory, by beta's id.
    let copy = t.rig.home.path().join("teams/beta/comms/m001.md");
    assert_eq!(
        std::fs::read_to_string(copy).unwrap(),
        "the users table moved to schema v2"
    );
}

#[test]
fn a_question_to_another_teams_lead_is_answered_back_across_teams() {
    let mut t = two_teams();
    let req = t.rig.request(
        "api",
        &["lead@beta"],
        MessageType::Question,
        "may I drop the legacy column?",
    );
    let asked = send(&mut t.rig.ctx(), req).unwrap();
    assert_eq!(asked.deliveries[0].state, RecipientState::HandedOff);
    let line = &t.typed("lead")[0];
    // Another team's lead answers with its own team named and the asker's
    // full name, and is told it is not an instruction.
    assert!(
        line.contains("teamwire send beta --from lead --to api@alpha --type answer --re m001"),
        "{line}"
    );
    assert!(line.contains("from team alpha"), "{line}");

    let mut answer = beta_request("lead", &["api@alpha"], MessageType::Answer, "yes, drop it");
    answer.re = Some(id("m001"));
    let answered = t.send_from_beta(answer).unwrap();
    assert_eq!(answered.deliveries[0].state, RecipientState::HandedOff);

    let typed = t.rig.host.submitted(&t.api);
    assert_eq!(typed.len(), 1);
    assert!(
        typed[0].starts_with("[teamwire:alpha msg m002 answer from lead@beta]"),
        "{}",
        typed[0]
    );
    // Another team's lead is a peer here, never "the lead".
    assert!(
        !typed[0].contains("This message is from the lead"),
        "{}",
        typed[0]
    );
    // The answer points at the same question in both stores.
    let here = messages(&t.rig.store);
    let there = messages(&t.beta);
    assert_eq!(here[1].re, Some(here[0].ulid));
    assert_eq!(there[1].re, Some(there[0].ulid));
    assert_eq!(here[0].ulid, there[0].ulid);
}

#[test]
fn a_member_answers_a_question_from_another_team() {
    let mut t = two_teams();
    let req = beta_request("db", &["api@alpha"], MessageType::Question, "which shape?");
    t.send_from_beta(req).unwrap();
    let mut answer = t
        .rig
        .request("api", &["db@beta"], MessageType::Answer, "the flat one");
    answer.re = Some(id("m001"));
    let report = send(&mut t.rig.ctx(), answer).unwrap();
    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
    let typed = t.typed("db");
    assert!(typed[0].contains("answer from api@alpha"), "{}", typed[0]);
    // An answer goes only to the asker, in whatever team.
    let mut stray = t
        .rig
        .request("api", &["scout@beta"], MessageType::Answer, "x");
    stray.re = Some(id("m001"));
    assert_eq!(
        refused(send(&mut t.rig.ctx(), stray)),
        Some(Refusal::RecipientNotAllowed)
    );
}

#[test]
fn another_team_is_refused_when_it_cannot_take_the_message() {
    let mut t = two_teams();
    let cases: [(&str, &[&str], MessageType, Refusal); 6] = [
        (
            "api",
            &["db@gamma"],
            MessageType::Finding,
            Refusal::UnknownTeam,
        ),
        (
            "api",
            &["nobody@beta"],
            MessageType::Finding,
            Refusal::UnknownRecipient,
        ),
        // A reviewer hears only its own lead, never another team's.
        (
            "api",
            &["rev@beta"],
            MessageType::Finding,
            Refusal::RecipientNotAllowed,
        ),
        (
            "lead",
            &["rev@beta"],
            MessageType::Finding,
            Refusal::RecipientNotAllowed,
        ),
        // Another team informs; it never instructs.
        (
            "lead",
            &["db@beta"],
            MessageType::Reprompt,
            Refusal::RecipientNotAllowed,
        ),
        (
            "lead",
            &["db@beta"],
            MessageType::Note,
            Refusal::RecipientNotAllowed,
        ),
    ];
    for (from, targets, kind, refusal) in cases {
        let req = t.rig.request(from, targets, kind, "x");
        assert_eq!(
            refused(send(&mut t.rig.ctx(), req)),
            Some(refusal),
            "{from} -> {targets:?} {kind}"
        );
    }
    // A reviewer of this team talks to no other team either.
    let req = t
        .rig
        .request("arev", &["db@beta"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut t.rig.ctx(), req)),
        Some(Refusal::TypeNotAllowed)
    );
    assert!(messages(&t.rig.store).is_empty());
    assert!(messages(&t.beta).is_empty());
}

#[test]
fn a_closed_team_takes_nothing() {
    let mut t = two_teams();
    let pid = teamwire_core::Pid::new(4242);
    t.beta.write(|tx| tx.begin_close(t.rig.now(), pid)).unwrap();
    let req = t
        .rig
        .request("api", &["db@beta"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut t.rig.ctx(), req)),
        Some(Refusal::RecipientTeamClosed)
    );
    assert!(messages(&t.rig.store).is_empty());
}

#[test]
fn a_name_that_points_back_at_the_own_team_is_the_local_name() {
    let mut t = two_teams();
    let req = t.rig.request(
        "lead",
        &["api@alpha", "api"],
        MessageType::Finding,
        "look at this",
    );
    let report = send(&mut t.rig.ctx(), req).unwrap();
    assert_eq!(report.deliveries.len(), 1);
    assert_eq!(report.deliveries[0].to.as_str(), "api");
    assert!(messages(&t.beta).is_empty());
}

#[test]
fn the_burst_limit_counts_messages_to_another_team() {
    let mut t = two_teams();
    for n in 0..5 {
        let req = t
            .rig
            .request("api", &["db@beta"], MessageType::Finding, &format!("f{n}"));
        send(&mut t.rig.ctx(), req).unwrap();
    }
    let req = t
        .rig
        .request("api", &["db@beta"], MessageType::Finding, "f5");
    assert_eq!(
        refused(send(&mut t.rig.ctx(), req)),
        Some(Refusal::RateLimited)
    );
}

/// A message stored in alpha as a send leaves it before anything reached
/// beta: the sender died right after its own commit.
fn stored_in_alpha_only(t: &mut Two, body: &str) -> Ulid {
    let at = t.rig.now();
    let stored = t
        .rig
        .store
        .write(|tx| {
            tx.insert_message(
                at,
                &NewMessage {
                    sender: Actor::Member(role("api")),
                    kind: MessageType::Finding,
                    re: None,
                    commit: None,
                    body: body.to_owned(),
                    client_key: None,
                    recipients: vec![to("db@beta")],
                },
            )
        })
        .unwrap();
    stored.message.ulid
}

#[test]
fn a_resend_writes_the_missing_copy_and_types_it_once() {
    let mut t = two_teams();
    let ulid = stored_in_alpha_only(&mut t, "copy me");
    t.rig.clock.advance(Duration::from_secs(61));
    let actor = Actor::Member(role("api"));
    let report = resend(&mut t.rig.ctx(), &actor, &id("m001"), None).unwrap();
    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
    assert_eq!(state_in(&t.beta, &ulid, "db"), DeliveryState::HandedOff);
    assert_eq!(t.typed("db").len(), 1);
    // Nothing is left to resend, and nothing is typed again.
    t.rig.clock.advance(Duration::from_secs(61));
    assert!(resend(&mut t.rig.ctx(), &actor, &id("m001"), None).is_err());
    assert_eq!(t.typed("db").len(), 1);
}

#[test]
fn a_resend_after_the_line_arrived_only_records_it() {
    let mut t = two_teams();
    let ulid = stored_in_alpha_only(&mut t, "already there");
    // The sender typed the line in beta and died before recording it here.
    let at = t.rig.now();
    t.beta
        .write(|tx| {
            tx.insert_received(
                at,
                &ReceivedMessage {
                    ulid,
                    sender: Remote::member_of(&TeamName::parse("alpha").unwrap(), &role("api")),
                    kind: MessageType::Finding,
                    re: None,
                    commit: None,
                    body: "already there".to_owned(),
                    recipients: vec![Recipient::Member(role("db"))],
                },
            )?;
            tx.set_delivery_state(
                at,
                &ulid,
                &to("db"),
                &teamwire_store::DeliveryUpdate {
                    state: DeliveryState::HandedOff,
                    route: Some(teamwire_core::Route::HostTyping),
                    error: None,
                    attempts: 1,
                },
            )
        })
        .unwrap();
    t.rig.clock.advance(Duration::from_secs(61));
    let actor = Actor::Member(role("api"));
    let report = resend(&mut t.rig.ctx(), &actor, &id("m001"), None).unwrap();
    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
    assert!(t.typed("db").is_empty());
    assert_eq!(messages(&t.beta).len(), 1);
}

#[test]
fn a_team_that_closed_after_the_send_was_stored_leaves_it_undelivered() {
    let mut t = two_teams();
    stored_in_alpha_only(&mut t, "too late");
    let pid = teamwire_core::Pid::new(4242);
    t.beta.write(|tx| tx.begin_close(t.rig.now(), pid)).unwrap();
    t.rig.clock.advance(Duration::from_secs(61));
    let actor = Actor::Member(role("api"));
    let report = resend(&mut t.rig.ctx(), &actor, &id("m001"), None).unwrap();
    let only = &report.deliveries[0];
    assert_eq!(only.state, RecipientState::Undelivered);
    assert_eq!(only.reason, Some(DeliveryErrorCode::TeamClosed));
    assert!(messages(&t.beta).is_empty());
}

#[test]
fn an_unreachable_member_is_reached_by_a_later_resend() {
    let mut t = two_teams();
    t.rig.set_reachable_session("session-beta-db", false);
    let req = t
        .rig
        .request("api", &["db@beta"], MessageType::Finding, "retry me");
    let report = send(&mut t.rig.ctx(), req).unwrap();
    assert_eq!(report.deliveries[0].state, RecipientState::Undelivered);
    let ulid = report.id;
    assert_eq!(state_in(&t.beta, &ulid, "db"), DeliveryState::Undelivered);

    t.rig.set_reachable_session("session-beta-db", true);
    let actor = Actor::Member(role("api"));
    let again = resend(&mut t.rig.ctx(), &actor, &id("m001"), None).unwrap();
    assert_eq!(again.deliveries[0].state, RecipientState::HandedOff);
    assert_eq!(state_in(&t.beta, &ulid, "db"), DeliveryState::HandedOff);
    assert_eq!(t.typed("db").len(), 1);
    // Beta counted both attempts on its own row.
    let attempts = t
        .beta
        .read(|tx| tx.delivery(&ulid, &to("db")))
        .unwrap()
        .unwrap()
        .delivery
        .attempts;
    assert_eq!(attempts, 2);
    // The retry is written in beta as a resend, so beta's own stale clock
    // restarts and a resend from beta cannot type it a second time.
    let resent = t
        .beta
        .read(|tx| {
            tx.events_of_type(EventType::DeliveryResent)?
                .all_or_corrupt()
        })
        .unwrap();
    assert_eq!(resent.len(), 1);
    assert_eq!(resent[0].data["to"], "db");
}

#[test]
fn each_team_logs_its_own_copy_and_only_beta_may_resend_it_there() {
    let mut t = two_teams();
    t.rig.set_reachable_session("session-beta-db", false);
    let req = t
        .rig
        .request("api", &["db@beta"], MessageType::Finding, "logged twice");
    send(&mut t.rig.ctx(), req).unwrap();

    let here = log(
        &t.rig.ctx(),
        LogFilter {
            viewer: Actor::Lead,
            role: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(here[0].to[0].as_str(), "db@beta");
    let there = log(
        &t.rig.ctx_on(&mut t.beta),
        LogFilter {
            viewer: Actor::Member(role("db")),
            role: None,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(there[0].from.as_str(), "api@alpha");

    // In beta, a member that did not send it may not resend; beta's lead may.
    let db = Actor::Member(role("db"));
    let err = resend(&mut t.rig.ctx_on(&mut t.beta), &db, &id("m001"), None).unwrap_err();
    assert_eq!(err, SendError::Refused(Refusal::NotResendable));
    t.rig.set_reachable_session("session-beta-db", true);
    let report = resend(
        &mut t.rig.ctx_on(&mut t.beta),
        &Actor::Lead,
        &id("m001"),
        None,
    )
    .unwrap();
    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
}

/// Beta holds alpha's message for `db` in `state`, and then starts closing.
fn held_by_a_closing_beta(t: &mut Two, text: &str, state: Option<DeliveryState>) -> Ulid {
    let ulid = stored_in_alpha_only(t, text);
    let at = t.rig.now();
    t.beta
        .write(|tx| {
            tx.insert_received(
                at,
                &ReceivedMessage {
                    ulid,
                    sender: Remote::member_of(&TeamName::parse("alpha").unwrap(), &role("api")),
                    kind: MessageType::Finding,
                    re: None,
                    commit: None,
                    body: text.to_owned(),
                    recipients: vec![Recipient::Member(role("db"))],
                },
            )?;
            if let Some(state) = state {
                tx.set_delivery_state(
                    at,
                    &ulid,
                    &to("db"),
                    &teamwire_store::DeliveryUpdate {
                        state,
                        route: Some(teamwire_core::Route::HostTyping),
                        error: None,
                        attempts: 1,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let pid = teamwire_core::Pid::new(4242);
    t.beta.write(|tx| tx.begin_close(t.rig.now(), pid)).unwrap();
    t.rig.clock.advance(Duration::from_secs(61));
    ulid
}

#[test]
fn a_line_that_arrived_stays_handed_off_after_the_other_team_starts_closing() {
    let mut t = two_teams();
    let ulid = held_by_a_closing_beta(&mut t, "already there", Some(DeliveryState::HandedOff));
    let actor = Actor::Member(role("api"));
    let report = resend(&mut t.rig.ctx(), &actor, &id("m001"), None).unwrap();
    assert_eq!(state_in(&t.beta, &ulid, "db"), DeliveryState::HandedOff);
    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
    assert_eq!(
        state_in(&t.rig.store, &ulid, "db@beta"),
        DeliveryState::HandedOff
    );
    assert!(t.typed("db").is_empty());
}

#[test]
fn a_copy_still_pending_in_a_closing_team_is_unconfirmed() {
    let mut t = two_teams();
    let ulid = held_by_a_closing_beta(&mut t, "maybe there", None);
    let actor = Actor::Member(role("api"));
    let report = resend(&mut t.rig.ctx(), &actor, &id("m001"), None).unwrap();
    let only = &report.deliveries[0];
    assert_eq!(only.state, RecipientState::Unconfirmed);
    assert_eq!(only.reason, Some(DeliveryErrorCode::TeamClosed));
    assert_eq!(
        state_in(&t.rig.store, &ulid, "db@beta"),
        DeliveryState::Unconfirmed
    );
    assert!(t.typed("db").is_empty());
}

/// The `store.error` rows of the team's store: `{op, code}` of each.
fn store_errors(store: &Store) -> Vec<serde_json::Value> {
    store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventType::StoreError)
        .map(|e| e.data)
        .collect()
}

/// Damages a row of beta's members as the recipient's copy is stored, which
/// is after the sender's rules read beta read-only, and before the delivery
/// reads the row. The row is valid JSON, so the CHECK passes, but not a list
/// of paths.
fn damage_beta_on_receive(t: &mut Two) {
    t.beta
        .write(|tx| {
            tx.connection().execute_batch(
                "CREATE TRIGGER damage AFTER INSERT ON messages
                 BEGIN UPDATE members SET owns = '{}'; END",
            )?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_send_that_fails_in_the_other_teams_store_leaves_its_row_there() {
    let mut t = two_teams();
    damage_beta_on_receive(&mut t);
    let req = t
        .rig
        .request("api", &["db@beta"], MessageType::Finding, "hello");
    assert!(send(&mut t.rig.ctx(), req).is_ok());
    assert_eq!(
        store_errors(&t.beta),
        [serde_json::json!({ "op": "send", "code": "corrupt" })]
    );
    assert!(store_errors(&t.rig.store).is_empty());
}

#[test]
fn a_resend_that_fails_in_the_other_teams_store_leaves_its_row_there() {
    let mut t = two_teams();
    stored_in_alpha_only(&mut t, "copy me");
    damage_beta_on_receive(&mut t);
    t.rig.clock.advance(Duration::from_secs(61));
    let actor = Actor::Member(role("api"));
    assert!(resend(&mut t.rig.ctx(), &actor, &id("m001"), None).is_ok());
    assert_eq!(
        store_errors(&t.beta),
        [serde_json::json!({ "op": "resend", "code": "corrupt" })]
    );
    assert!(store_errors(&t.rig.store).is_empty());
}
