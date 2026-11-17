//! `lead_missed` (ADR 0051): which results and messages to the lead fall in
//! the window of a lead handoff. The window starts 120 seconds before the
//! old lead's document was written and ends at the adoption.
#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "a failing unwrap in an integration test fails the test, and the test writes the handoff document an old lead would"
)]

use std::time::{Duration, UNIX_EPOCH};

use teamwire_core::{
    Actor, Clock, CompletionStatus, DeliveryState, FinalSource, HarnessId, HostId, LeadRoute,
    MemberKind, MessageType, Mode, Recipient, ReportState, Role, SessionId, SessionKind, TeamName,
    Timestamp, Ulid,
};
use teamwire_fakes::{FakeClock, TestHome};
use teamwire_report::lead_missed;
use teamwire_store::{
    DeliveryUpdate, NewCompletion, NewMember, NewMessage, NewTeam, ResultOutcome, Store,
};

struct Rig {
    home: TestHome,
    store: Store,
    t0: Timestamp,
}

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn api() -> Role {
    Role::parse("api").unwrap()
}

fn alpha() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

impl Rig {
    /// Team `alpha`, led by `lead-1`, with member `api`.
    fn new() -> Self {
        let home = TestHome::new().unwrap();
        let t0 = FakeClock::new().now();
        let mut store = Store::open(home.path(), &alpha()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    t0,
                    &NewTeam {
                        name: alpha(),
                        title: "alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((sid("lead-1"), SessionKind::Interactive)),
                    },
                )?;
                tx.insert_member(
                    t0,
                    &NewMember {
                        role: api(),
                        title: "api".to_owned(),
                        kind: MemberKind::Implementer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/work".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )
            })
            .unwrap();
        Self { home, store, t0 }
    }

    fn at(&self, minutes: u64) -> Timestamp {
        self.t0
            .checked_add(Duration::from_secs(minutes * 60))
            .unwrap()
    }

    /// The new lead `lead-2` adopts the team at minute 60.
    fn adopt(&mut self) {
        self.adopt_at(60, "lead-2");
    }

    fn adopt_at(&mut self, minutes: u64, session: &str) {
        let at = self.at(minutes);
        self.store
            .write(|tx| tx.adopt_lead(at, &sid(session), SessionKind::Interactive))
            .unwrap();
    }

    /// The old lead's handoff document, last written at `minutes`.
    fn document(&self, minutes: u64) {
        let dir = self.home.path().join("handoffs").join("lead-1");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("handoff.md");
        std::fs::write(&path, "# handoff\n").unwrap();
        let millis = u64::try_from(self.at(minutes).unix_millis()).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_millis(millis))
            .unwrap();
    }

    /// A result of `api` made at `made`, delivered to the lead at `delivered`.
    fn result(&mut self, made: u64, delivered: Option<u64>) {
        let (made, delivered) = (self.at(made), delivered.map(|m| self.at(m)));
        self.store
            .write(|tx| {
                let c = tx.insert_completion(
                    made,
                    &NewCompletion {
                        role: api(),
                        generation: 1,
                        status: CompletionStatus::Done,
                        summary: "done".to_owned(),
                        final_message: None,
                        final_source: FinalSource::None,
                    },
                )?;
                if let Some(at) = delivered {
                    tx.claim_completion_for_delivery(at, &c.ulid, Duration::from_secs(90))?;
                    tx.finish_completion_delivery(
                        at,
                        &c.ulid,
                        &ResultOutcome {
                            state: ReportState::Delivered,
                            route: None,
                            error: None,
                            result_sha256: None,
                        },
                    )?;
                }
                Ok(())
            })
            .unwrap();
    }

    /// A finding from `api` to the lead, made at `made`, then moved to each
    /// `(minute, state)` in turn.
    fn message(&mut self, made: u64, moves: &[(u64, DeliveryState)]) -> Ulid {
        let moves: Vec<(u64, Recipient, DeliveryState)> = moves
            .iter()
            .map(|(m, s)| (*m, Recipient::Lead, *s))
            .collect();
        self.message_to(made, &[Recipient::Lead], &moves)
    }

    /// A finding to `recipients`, then each `(minute, recipient, state)`.
    fn message_to(
        &mut self,
        made: u64,
        recipients: &[Recipient],
        moves: &[(u64, Recipient, DeliveryState)],
    ) -> Ulid {
        let made = self.at(made);
        let moves: Vec<(Timestamp, Recipient, DeliveryState)> = moves
            .iter()
            .map(|(m, r, s)| (self.at(*m), r.clone(), *s))
            .collect();
        self.store
            .write(|tx| {
                let id = tx
                    .insert_message(
                        made,
                        &NewMessage {
                            sender: Actor::Member(api()),
                            kind: MessageType::Finding,
                            re: None,
                            commit: None,
                            body: "a finding".to_owned(),
                            client_key: None,
                            recipients: recipients.to_vec(),
                        },
                    )?
                    .message
                    .ulid;
                for (at, to, state) in &moves {
                    if *state == DeliveryState::Acked {
                        tx.ack_delivery(*at, &id, to)?;
                    } else {
                        tx.set_delivery_state(
                            *at,
                            &id,
                            to,
                            &DeliveryUpdate {
                                state: *state,
                                route: None,
                                error: None,
                                attempts: 1,
                            },
                        )?;
                    }
                }
                Ok(id)
            })
            .unwrap()
    }

    fn missed(&self) -> Vec<String> {
        lead_missed(self.home.path(), &sid("lead-2"), &[alpha()]).unwrap()
    }

    fn outbox(&self) -> String {
        self.home
            .path()
            .join("teams/alpha/reports/api.md")
            .display()
            .to_string()
    }

    fn comms(&self, id: &str) -> String {
        self.home
            .path()
            .join("teams/alpha/comms")
            .join(format!("{id}.md"))
            .display()
            .to_string()
    }
}

#[test]
fn an_item_is_listed_when_its_newest_delivery_falls_in_the_window_or_it_has_none() {
    let mut rig = Rig::new();
    // The document was written at minute 55, so the window is 53 to 60.
    rig.result(10, Some(11));
    // Made 15 minutes before the document, typed after it was written.
    rig.result(40, Some(57));
    rig.message(20, &[(21, DeliveryState::HandedOff)]);
    rig.message(
        20,
        &[(21, DeliveryState::HandedOff), (54, DeliveryState::Acked)],
    );
    rig.message(50, &[]);
    // The member's ack is in the window; the lead's line is not.
    rig.message_to(
        20,
        &[Recipient::Lead, Recipient::Member(api())],
        &[
            (21, Recipient::Lead, DeliveryState::HandedOff),
            (58, Recipient::Member(api()), DeliveryState::Acked),
        ],
    );
    rig.adopt();
    rig.message(61, &[]);
    rig.result(62, None);
    rig.document(55);

    assert_eq!(
        rig.missed(),
        [
            format!(
                "team alpha: api's result #2 (done), result.delivered at {}: read {}",
                rig.at(57),
                rig.outbox()
            ),
            format!(
                "team alpha: message m002 (finding from api), delivery.acked at {}: read {}",
                rig.at(54),
                rig.comms("m002")
            ),
            format!(
                "team alpha: message m003 (finding from api), not delivered: read {}",
                rig.comms("m003")
            ),
        ]
    );
}

#[test]
fn without_the_document_the_window_starts_an_hour_before_the_handoff() {
    let mut rig = Rig::new();
    rig.result(10, Some(11));
    rig.message(0, &[(0, DeliveryState::HandedOff)]);
    rig.adopt();
    let lines = rig.missed();
    assert_eq!(
        lines,
        [
            "team alpha: the old lead's handoff document is gone, so the window starts one hour \
             before the handoff"
                .to_owned(),
            format!(
                "team alpha: api's result #1 (done), result.delivered at {}: read {}",
                rig.at(11),
                rig.outbox()
            ),
            format!(
                "team alpha: message m001 (finding from api), delivery.handed_off at {}: read {}",
                rig.at(0),
                rig.comms("m001")
            ),
        ]
    );
}

#[test]
fn nothing_in_the_window_is_one_line() {
    let mut rig = Rig::new();
    rig.result(10, Some(11));
    rig.adopt();
    rig.document(55);
    assert_eq!(
        rig.missed(),
        ["nothing was missed: no result or message reached the old lead while it handed off"]
    );
}

#[test]
fn a_team_with_no_handoff_to_this_session_says_so() {
    let mut rig = Rig::new();
    rig.result(10, None);
    assert_eq!(
        rig.missed(),
        [
            "team alpha: no handoff to this session is recorded",
            "nothing was missed: no result or message reached the old lead while it handed off",
        ]
    );
}

#[test]
fn the_newest_handoff_to_this_session_sets_the_window() {
    let mut rig = Rig::new();
    rig.adopt_at(30, "lead-2");
    rig.adopt_at(45, "lead-1");
    rig.result(40, Some(57));
    rig.adopt();
    rig.document(55);
    assert_eq!(
        rig.missed(),
        [format!(
            "team alpha: api's result #1 (done), result.delivered at {}: read {}",
            rig.at(57),
            rig.outbox()
        )]
    );
}

#[test]
fn an_item_made_before_the_adoption_and_delivered_after_it_is_still_listed() {
    let mut rig = Rig::new();
    rig.adopt();
    // Both were made before the handoff, and both reached a lead after it.
    rig.result(50, Some(65));
    rig.message(
        56,
        &[(57, DeliveryState::HandedOff), (70, DeliveryState::Acked)],
    );
    rig.document(55);
    assert_eq!(
        rig.missed(),
        [
            format!(
                "team alpha: api's result #1 (done), result.delivered at {}: read {}",
                rig.at(65),
                rig.outbox()
            ),
            format!(
                "team alpha: message m001 (finding from api), delivery.acked at {}: read {}",
                rig.at(70),
                rig.comms("m001")
            ),
        ]
    );
}
