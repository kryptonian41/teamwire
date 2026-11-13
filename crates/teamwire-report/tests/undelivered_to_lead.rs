//! The lines the lead's hooks add for each item that was never typed into
//! the lead's pane (ADR 0043 decision 1): a question to the lead and a
//! result, each with the file to read and the command that clears it.
#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "a failing unwrap in an integration test fails the test, and the test writes the result file a member's hook would write"
)]

use std::time::Duration;

use teamwire_core::{
    Actor, Clock, CompletionStatus, DeliveryError, DeliveryErrorCode, DeliveryState, FinalSource,
    HarnessId, HostId, LeadRoute, MemberKind, MemberState, MessageType, Mode, Recipient,
    ReportState, Role, SessionId, SessionKind, TeamName, Ulid,
};
use teamwire_fakes::{FakeClock, FakePlatform, TestHome};
use teamwire_report::undelivered_to_lead;
use teamwire_store::{
    DeliveryUpdate, NewCompletion, NewMember, NewMessage, NewTeam, ResultOutcome, Store,
};

const RESULT: &[u8] = b"the result";

struct Rig {
    home: TestHome,
    platform: FakePlatform,
    clock: FakeClock,
    store: Store,
}

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn api() -> Role {
    Role::parse("api").unwrap()
}

impl Rig {
    /// Team `alpha`, led from `lead-1`, with member `api` started.
    fn new() -> Self {
        let home = TestHome::new().unwrap();
        let clock = FakeClock::new();
        let mut store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
        let now = clock.now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: TeamName::parse("alpha").unwrap(),
                        title: "alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((sid("lead-1"), SessionKind::Interactive)),
                    },
                )?;
                tx.insert_member(
                    now,
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
                )?;
                tx.set_member_state(now, &api(), MemberState::Started, &serde_json::json!({}))
            })
            .unwrap();
        Self {
            home,
            platform: FakePlatform::new(),
            clock,
            store,
        }
    }

    fn team_dir(&self) -> std::path::PathBuf {
        self.home.path().join("teams").join("alpha")
    }

    fn lines(&self) -> Vec<String> {
        undelivered_to_lead(
            self.home.path(),
            &sid("lead-1"),
            &self.platform,
            &self.clock,
        )
    }

    /// A question from `api` to the lead that a blocked pane refused.
    fn blocked_question(&mut self) -> Ulid {
        let now = self.clock.now();
        self.store
            .write(|tx| {
                let stored = tx.insert_message(
                    now,
                    &NewMessage {
                        sender: Actor::Member(api()),
                        kind: MessageType::Question,
                        re: None,
                        commit: None,
                        body: "which base?".to_owned(),
                        client_key: None,
                        recipients: vec![Recipient::Lead],
                    },
                )?;
                let id = stored.message.ulid;
                tx.set_delivery_state(
                    now,
                    &id,
                    &Recipient::Lead,
                    &DeliveryUpdate {
                        state: DeliveryState::Undelivered,
                        route: None,
                        error: Some(DeliveryError::new(
                            DeliveryErrorCode::SessionBlocked,
                            "a dialog is open",
                        )),
                        attempts: 1,
                    },
                )?;
                Ok(id)
            })
            .unwrap()
    }

    /// `api`'s result #1, whose typing was refused. With `file`, the result
    /// file holds the recorded text.
    fn blocked_result(&mut self, file: bool) {
        if file {
            let outbox = self.team_dir().join("outbox");
            std::fs::create_dir_all(&outbox).unwrap();
            std::fs::write(outbox.join("api.md"), RESULT).unwrap();
        }
        let now = self.clock.now();
        self.store
            .write(|tx| {
                let c = tx.insert_completion(
                    now,
                    &NewCompletion {
                        role: api(),
                        generation: 1,
                        status: CompletionStatus::Done,
                        summary: "done".to_owned(),
                        final_message: None,
                        final_source: FinalSource::None,
                    },
                )?;
                let id = c.ulid;
                tx.claim_completion_for_delivery(now, &id, Duration::from_secs(90))?;
                tx.finish_completion_delivery(
                    now,
                    &id,
                    &ResultOutcome {
                        state: ReportState::Undelivered,
                        route: None,
                        error: Some(DeliveryError::new(
                            DeliveryErrorCode::SessionBlocked,
                            "a dialog is open",
                        )),
                        result_sha256: Some(teamwire_store::sha256(RESULT)),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    }
}

#[test]
fn a_question_and_a_result_never_typed_are_listed_with_their_files() {
    let mut rig = Rig::new();
    rig.blocked_question();
    rig.blocked_result(true);
    let lines = rig.lines();
    let dir = rig.team_dir();
    let comms = dir.join("comms").join("m001.md").display().to_string();
    let outbox = dir.join("outbox").join("api.md").display().to_string();
    assert!(
        lines.iter().any(|l| l.contains("m001 (question from api)")
            && l.contains("was never typed into your pane")
            && l.contains(&comms)
            && l.contains("teamwire received alpha m001")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("api's result #1")
            && l.contains("was never typed into your pane")
            && l.contains(&outbox)
            && l.contains("teamwire received alpha api")),
        "{lines:?}"
    );
    assert_eq!(lines.len(), 2, "{lines:?}");
}

#[test]
fn a_row_the_lead_received_is_not_listed() {
    let mut rig = Rig::new();
    let question = rig.blocked_question();
    rig.blocked_result(true);
    assert_eq!(rig.lines().len(), 2, "{:?}", rig.lines());
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.ack_delivery(now, &question, &Recipient::Lead)?;
            let newest = tx.reader().latest_completion(&api())?.unwrap();
            tx.mark_result_received(now, &newest.completion.ulid)
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(rig.lines(), Vec::<String>::new());
}

#[test]
fn a_result_with_no_result_file_names_the_report() {
    let mut rig = Rig::new();
    rig.blocked_result(false);
    let report = rig
        .team_dir()
        .join("reports")
        .join("api.md")
        .display()
        .to_string();
    let lines = rig.lines();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("api's result #1") && l.contains(&report)),
        "{lines:?}"
    );
}
