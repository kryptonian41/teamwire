//! The rules of a send and the outbox order: refusals store nothing, a send
//! is committed before any host call, and each recipient has its own state.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use std::cell::RefCell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::json;
use teamwire_core::{
    Clock, DeliveryErrorCode, EventType, HostError, HostErrorCode, MemberKind, MessageType, Pid,
    Refusal, RunOutput, SendRequest, Timestamp,
};
use teamwire_fakes::Fault;
use teamwire_msg::{RecipientState, SendError, SendReport, send, send_with, send_with_write};
use teamwire_store::{Ctx, Store};

use rig::{Rig, id, role, to};

const FULL_SHA: &str = "3f2a9c1e5b7d40a8c6d2e1f0a9b8c7d6e5f40312";

fn team_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    rig.add_member("web", MemberKind::Implementer);
    rig.add_member("scout", MemberKind::Investigator);
    rig.add_member("close", MemberKind::Closer);
    rig.add_member("rev", MemberKind::Reviewer);
    rig
}

/// The refusal a send ended in, or `None` when it ended any other way.
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

/// The only recipient's report of a send to one recipient.
fn only(report: &teamwire_msg::SendReport) -> &teamwire_msg::RecipientReport {
    assert_eq!(report.deliveries.len(), 1);
    report.deliveries.first().unwrap()
}

fn finished(exit_code: i32, stdout: &str) -> RunOutput {
    RunOutput {
        exit_code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out: false,
    }
}

fn nothing_stored(rig: &Rig) -> bool {
    rig.store
        .read(|tx| {
            Ok(tx
                .messages(&teamwire_store::MessageFilter::default())?
                .items
                .is_empty()
                && tx.counter("display_id")?.is_none())
        })
        .unwrap()
}

// ---- refusals ----

#[test]
fn notice_with_a_real_commit_is_delivered_and_stores_the_full_hash() {
    let mut rig = team_rig();
    rig.platform
        .queue_run(Ok(finished(0, &format!("{FULL_SHA}\n"))));
    rig.platform.queue_run(Ok(finished(0, "")));
    let mut req = rig.request(
        "api",
        &["web"],
        MessageType::Notice,
        "renamed wrap1 to bracket1",
    );
    req.commit = Some("3f2a9c1".to_owned());
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(only(&report).state, RecipientState::HandedOff);
    let stored = rig
        .store
        .read(|tx| tx.message_by_display_id(&id("m001")))
        .unwrap()
        .unwrap();
    assert_eq!(stored.commit.as_deref(), Some(FULL_SHA));
    // Git ran in the sender's own directory: rev-parse, then merge-base.
    let runs = rig.platform.runs();
    assert_eq!(runs.len(), 2, "{runs:?}");
    let (rev_parse, _) = runs.first().unwrap();
    let (merge_base, _) = runs.last().unwrap();
    assert!(rev_parse.args.contains(&"3f2a9c1^{commit}".to_owned()));
    assert_eq!(
        rev_parse.cwd.as_deref(),
        Some(rig.home.path().join("work/api").as_path())
    );
    assert_eq!(
        merge_base.args,
        ["merge-base", "--is-ancestor", FULL_SHA, "HEAD"]
    );
}

#[test]
fn notice_without_a_commit_is_refused() {
    let mut rig = team_rig();
    let req = rig.request(
        "api",
        &["web"],
        MessageType::Notice,
        "I will rename it later",
    );
    assert_eq!(refused(send(&mut rig.ctx(), req)), Some(Refusal::NoCommit));
    assert!(nothing_stored(&rig));
}

#[test]
fn notice_naming_a_commit_that_is_missing_or_not_on_the_branch_is_refused() {
    let mut rig = team_rig();
    let mut req = rig.request("api", &["web"], MessageType::Notice, "x");
    req.commit = Some("deadbee".to_owned());
    rig.platform.queue_run(Ok(finished(1, "")));
    assert_eq!(
        refused(send(&mut rig.ctx(), req.clone())),
        Some(Refusal::CommitNotReachable)
    );
    // The commit exists, but is not an ancestor of the sender's head.
    rig.platform
        .queue_run(Ok(finished(0, &format!("{FULL_SHA}\n"))));
    rig.platform.queue_run(Ok(finished(1, "")));
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::CommitNotReachable)
    );
    assert!(nothing_stored(&rig));
}

#[test]
fn a_commit_that_looks_like_a_git_option_never_reaches_git() {
    let mut rig = team_rig();
    let mut req = rig.request("api", &["web"], MessageType::Notice, "x");
    req.commit = Some("--upload-pack=touch /tmp/x".to_owned());
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::CommitNotReachable)
    );
    assert!(rig.platform.runs().is_empty());
}

#[test]
fn reviewers_and_closers_send_nothing() {
    let mut rig = team_rig();
    for from in ["close", "rev"] {
        let req = rig.request(from, &["lead"], MessageType::Question, "x");
        assert_eq!(
            refused(send(&mut rig.ctx(), req)),
            Some(Refusal::TypeNotAllowed),
            "{from}"
        );
    }
    assert!(nothing_stored(&rig));
}

#[test]
fn members_may_not_message_a_closer_or_reviewer_but_the_lead_may() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["close"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::RecipientNotAllowed)
    );
    let req = rig.request("scout", &["rev"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::RecipientNotAllowed)
    );
    let req = rig.request("lead", &["close"], MessageType::Reprompt, "look again");
    assert!(send(&mut rig.ctx(), req).is_ok());
}

#[test]
fn a_question_goes_to_any_talking_agent_and_never_to_a_quiet_one() {
    let mut rig = team_rig();
    for target in ["web", "scout", "lead"] {
        let req = rig.request(
            "api",
            &[target],
            MessageType::Question,
            &format!("to {target}?"),
        );
        assert!(send(&mut rig.ctx(), req).is_ok(), "{target}");
    }
    for target in ["rev", "close"] {
        let req = rig.request("api", &[target], MessageType::Question, "x?");
        assert_eq!(
            refused(send(&mut rig.ctx(), req)),
            Some(Refusal::RecipientNotAllowed),
            "{target}"
        );
    }
}

/// The team rig with a plan reviewer, `plan`, beside the reviewer.
fn plan_rig() -> (Rig, teamwire_core::Place) {
    let mut rig = team_rig();
    let plan = rig.add_member("plan", MemberKind::PlanReviewer);
    (rig, plan)
}

#[test]
fn a_plan_reviewer_sends_nothing_of_any_type() {
    let (mut rig, _) = plan_rig();
    // A real question asked of the plan reviewer, so its answer fails on
    // the sender's kind and not on a missing question.
    let ask = rig.request("lead", &["plan"], MessageType::Question, "which brief?");
    send(&mut rig.ctx(), ask).unwrap();
    for kind in MessageType::ALL.iter().copied() {
        let mut req = rig.request("plan", &["lead"], kind, "x");
        req.commit = Some(FULL_SHA.to_owned());
        req.re = (kind == MessageType::Answer).then(|| id("m001"));
        assert_eq!(
            refused(send(&mut rig.ctx(), req)),
            Some(Refusal::TypeNotAllowed),
            "{kind}"
        );
    }
    // Only the lead's question is stored, and no git command ran.
    let stored = rig
        .store
        .read(|tx| {
            tx.messages(&teamwire_store::MessageFilter::default())?
                .all_or_corrupt()
        })
        .unwrap();
    let senders: Vec<&str> = stored.iter().map(|m| m.sender.as_str()).collect();
    assert_eq!(senders, ["lead"]);
    assert!(rig.platform.runs().is_empty());
}

#[test]
fn members_may_not_message_a_plan_reviewer() {
    let (mut rig, _) = plan_rig();
    for from in ["api", "scout"] {
        for kind in [
            MessageType::Notice,
            MessageType::Finding,
            MessageType::Question,
        ] {
            let mut req = rig.request(from, &["plan"], kind, "x");
            req.commit = Some(FULL_SHA.to_owned());
            assert_eq!(
                refused(send(&mut rig.ctx(), req)),
                Some(Refusal::RecipientNotAllowed),
                "{from} {kind}"
            );
        }
    }
    assert!(nothing_stored(&rig));
    assert!(rig.platform.runs().is_empty());
}

#[test]
fn the_leads_note_and_reprompt_reach_a_plan_reviewer() {
    let (mut rig, plan) = plan_rig();
    for kind in [MessageType::Note, MessageType::Reprompt] {
        let req = rig.request("lead", &["plan"], kind, &format!("a {kind} for the plan"));
        let report = send(&mut rig.ctx(), req).unwrap();
        assert_eq!(only(&report).state, RecipientState::HandedOff, "{kind}");
    }
    assert_eq!(rig.host.submitted(&plan).len(), 2);
}

/// The team rig with a reconciler, `rec`, and an investigator whose spec
/// set `quiet`, `hush`. The investigator's kind talks; its setting makes it
/// quiet.
fn quiet_rig() -> (Rig, [teamwire_core::Place; 2]) {
    let mut rig = team_rig();
    let rec = rig.add_member("rec", MemberKind::Reconciler);
    let hush = rig.add_member("hush", MemberKind::Investigator);
    rig.store
        .write(|tx| tx.set_member_marks(&role("hush"), true, None))
        .unwrap();
    (rig, [rec, hush])
}

const QUIET: [&str; 2] = ["rec", "hush"];

#[test]
fn a_reconciler_and_a_quiet_investigator_send_nothing_of_any_type() {
    let (mut rig, _) = quiet_rig();
    // A real question asked of each, so an answer fails on the sender and
    // not on a missing question.
    for (n, target) in QUIET.iter().enumerate() {
        let ask = rig.request("lead", &[target], MessageType::Question, &format!("q{n}?"));
        send(&mut rig.ctx(), ask).unwrap();
    }
    for (n, from) in QUIET.iter().enumerate() {
        for kind in MessageType::ALL.iter().copied() {
            let mut req = rig.request(from, &["lead"], kind, "x");
            req.commit = Some(FULL_SHA.to_owned());
            req.re = (kind == MessageType::Answer).then(|| id(&format!("m00{}", n + 1)));
            assert_eq!(
                refused(send(&mut rig.ctx(), req)),
                Some(Refusal::TypeNotAllowed),
                "{from} {kind}"
            );
        }
        // Not even to a member that talks.
        let req = rig.request(from, &["api"], MessageType::Finding, "x");
        assert_eq!(
            refused(send(&mut rig.ctx(), req)),
            Some(Refusal::TypeNotAllowed),
            "{from} to api"
        );
    }
    // Only the lead's two questions are stored, and no git command ran.
    let stored = rig
        .store
        .read(|tx| {
            tx.messages(&teamwire_store::MessageFilter::default())?
                .all_or_corrupt()
        })
        .unwrap();
    let senders: Vec<&str> = stored.iter().map(|m| m.sender.as_str()).collect();
    assert_eq!(senders, ["lead", "lead"]);
    assert!(rig.platform.runs().is_empty());
}

#[test]
fn members_may_not_message_a_reconciler_or_a_quiet_investigator() {
    let (mut rig, _) = quiet_rig();
    for target in QUIET {
        for from in ["api", "scout"] {
            for kind in [
                MessageType::Notice,
                MessageType::Finding,
                MessageType::Question,
            ] {
                let mut req = rig.request(from, &[target], kind, "x");
                req.commit = Some(FULL_SHA.to_owned());
                assert_eq!(
                    refused(send(&mut rig.ctx(), req)),
                    Some(Refusal::RecipientNotAllowed),
                    "{from} {kind} to {target}"
                );
            }
        }
    }
    assert!(nothing_stored(&rig));
    assert!(rig.platform.runs().is_empty());
}

#[test]
fn the_leads_note_and_reprompt_reach_a_reconciler_and_a_quiet_investigator() {
    let (mut rig, places) = quiet_rig();
    for (target, place) in QUIET.iter().zip(&places) {
        for kind in [MessageType::Note, MessageType::Reprompt] {
            let req = rig.request("lead", &[target], kind, &format!("a {kind} for {target}"));
            let report = send(&mut rig.ctx(), req).unwrap();
            assert_eq!(
                only(&report).state,
                RecipientState::HandedOff,
                "{kind} to {target}"
            );
        }
        assert_eq!(rig.host.submitted(place).len(), 2, "{target}");
    }
}

/// The setting is read when the message is sent: an investigator talks
/// until its row says `quiet`.
#[test]
fn an_investigator_talks_until_its_quiet_mark_is_set() {
    let mut rig = team_rig();
    rig.add_member("hush", MemberKind::Investigator);
    let req = rig.request("hush", &["api"], MessageType::Finding, "found it");
    assert!(send(&mut rig.ctx(), req).is_ok());
    rig.store
        .write(|tx| tx.set_member_marks(&role("hush"), true, None))
        .unwrap();
    let req = rig.request("hush", &["api"], MessageType::Finding, "found more");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::TypeNotAllowed)
    );
}

#[test]
fn only_the_lead_sends_a_reprompt_or_a_note() {
    let mut rig = team_rig();
    for kind in [MessageType::Reprompt, MessageType::Note] {
        let req = rig.request("api", &["web"], kind, "x");
        assert_eq!(
            refused(send(&mut rig.ctx(), req)),
            Some(Refusal::TypeNotAllowed),
            "{kind}"
        );
    }
}

#[test]
fn every_talking_kind_sends_every_kind_of_information() {
    let mut rig = team_rig();
    let req = rig.request("api", &["web"], MessageType::Finding, "found it");
    assert!(send(&mut rig.ctx(), req).is_ok());
    rig.platform
        .queue_run(Ok(finished(0, &format!("{FULL_SHA}\n"))));
    rig.platform.queue_run(Ok(finished(0, "")));
    let mut req = rig.request("scout", &["web"], MessageType::Notice, "moved");
    req.commit = Some(FULL_SHA.to_owned());
    assert!(send(&mut rig.ctx(), req).is_ok());
    for kind in [MessageType::Finding, MessageType::Question] {
        let req = rig.request("lead", &["web"], kind, &format!("lead {kind}"));
        assert!(send(&mut rig.ctx(), req).is_ok(), "{kind}");
    }
    // A notice names a commit in the sender's checkout; the lead has none.
    let mut req = rig.request("lead", &["web"], MessageType::Notice, "x");
    req.commit = Some(FULL_SHA.to_owned());
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::TypeNotAllowed)
    );
}

#[test]
fn size_limits_follow_the_type() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, &"y".repeat(801));
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::TooLong {
            limit: 800,
            got: 801
        })
    );
    let mut req = rig.request("api", &["web"], MessageType::Notice, &"y".repeat(601));
    req.commit = Some(FULL_SHA.to_owned());
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::TooLong {
            limit: 600,
            got: 601
        })
    );
    let req = rig.request("scout", &["api"], MessageType::Finding, &"y".repeat(800));
    assert!(send(&mut rig.ctx(), req).is_ok());
    // A reprompt has no limit, and the limit counts characters, not bytes.
    let req = rig.request("lead", &["web"], MessageType::Reprompt, &"y".repeat(5000));
    assert!(send(&mut rig.ctx(), req).is_ok());
    let req = rig.request("scout", &["web"], MessageType::Finding, &"é".repeat(800));
    assert!(send(&mut rig.ctx(), req).is_ok());
}

#[test]
fn an_empty_text_is_refused() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, "  \n\t ");
    assert_eq!(refused(send(&mut rig.ctx(), req)), Some(Refusal::EmptyBody));
    assert!(nothing_stored(&rig));
}

#[test]
fn recipients_must_be_other_current_members() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["scout"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::SelfMessage)
    );
    let req = rig.request("scout", &["ghost"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::UnknownRecipient)
    );
    let now = rig.now();
    rig.store
        .write(|tx| {
            tx.remove_member(now, &role("web"), None, &json!({}))
                .map(|_| ())
        })
        .unwrap();
    let req = rig.request("scout", &["web"], MessageType::Finding, "x");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::RemovedRecipient)
    );
    let req = rig.request("scout", &[], MessageType::Finding, "x");
    assert!(matches!(
        send(&mut rig.ctx(), req),
        Err(SendError::InvalidArgument(_))
    ));
    assert!(nothing_stored(&rig));
}

#[test]
fn an_answer_answers_a_question_asked_of_the_answerer_and_goes_to_the_asker() {
    let mut rig = team_rig();
    let q = rig.request(
        "api",
        &["scout"],
        MessageType::Question,
        "does v2 still call it?",
    );
    let asked = send(&mut rig.ctx(), q).unwrap();
    let mut answer = rig.request("scout", &["web"], MessageType::Answer, "yes");
    answer.re = Some(asked.display_id.clone());
    assert_eq!(
        refused(send(&mut rig.ctx(), answer.clone())),
        Some(Refusal::RecipientNotAllowed)
    );
    // Not asked of web: refused whoever the answer goes to.
    answer.actor = teamwire_core::Actor::parse("web").unwrap();
    answer.to = vec![to("api")];
    assert_eq!(
        refused(send(&mut rig.ctx(), answer.clone())),
        Some(Refusal::NotAskedOfYou)
    );
    answer.actor = teamwire_core::Actor::parse("scout").unwrap();
    assert!(send(&mut rig.ctx(), answer).is_ok());
}

#[test]
fn an_answer_needs_a_real_question() {
    let mut rig = team_rig();
    let note = rig.request("lead", &["scout"], MessageType::Note, "fyi");
    let noted = send(&mut rig.ctx(), note).unwrap();
    let mut answer = rig.request("scout", &["lead"], MessageType::Answer, "ok");
    assert!(matches!(
        send(&mut rig.ctx(), answer.clone()),
        Err(SendError::InvalidArgument(_))
    ));
    answer.re = Some(noted.display_id);
    assert_eq!(
        refused(send(&mut rig.ctx(), answer.clone())),
        Some(Refusal::NotAQuestion)
    );
    answer.re = Some(id("m099"));
    assert_eq!(
        refused(send(&mut rig.ctx(), answer)),
        Some(Refusal::NotAQuestion)
    );
}

#[test]
fn a_closing_team_takes_no_messages() {
    let mut rig = team_rig();
    let now = rig.now();
    rig.store
        .write(|tx| tx.begin_close(now, Pid::new(4242)).map(|_| ()))
        .unwrap();
    let req = rig.request("api", &["web"], MessageType::Finding, "x");
    let req = SendRequest {
        actor: teamwire_core::Actor::parse("lead").unwrap(),
        kind: MessageType::Note,
        ..req
    };
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::TeamClosed)
    );
    assert!(nothing_stored(&rig));
}

#[test]
fn a_refusal_is_logged_with_its_code_and_stores_nothing_else() {
    let mut rig = team_rig();
    let req = rig.request("api", &["web"], MessageType::Reprompt, "SECRET body text");
    assert!(send(&mut rig.ctx(), req).is_err());
    let events = rig.events(EventType::MessageRefused);
    assert_eq!(events.len(), 1, "{events:?}");
    let data = &events.first().unwrap().data;
    assert_eq!(data.get("code"), Some(&json!("type_not_allowed")));
    assert_eq!(data.get("sender"), Some(&json!("api")));
    assert!(!data.to_string().contains("SECRET"));
    assert!(nothing_stored(&rig));
    assert!(rig.events(EventType::MessageSent).is_empty());
}

// ---- burst and repeat (MSG-14) ----

#[test]
fn burst_refused_and_logged() {
    let mut rig = team_rig();
    for n in 0..5 {
        let req = rig.request(
            "scout",
            &["api"],
            MessageType::Finding,
            &format!("finding {n}"),
        );
        assert!(send(&mut rig.ctx(), req).is_ok(), "message {n}");
    }
    let req = rig.request("scout", &["api"], MessageType::Finding, "finding 5");
    assert_eq!(
        refused(send(&mut rig.ctx(), req)),
        Some(Refusal::RateLimited)
    );
    let events = rig.events(EventType::MessageRefused);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events.first().unwrap().data.get("code"),
        Some(&json!("rate_limited"))
    );
    // Another recipient is not affected, and a minute later the limit is over.
    let req = rig.request("scout", &["web"], MessageType::Finding, "finding 5");
    assert!(send(&mut rig.ctx(), req).is_ok());
    rig.clock.advance(Duration::from_secs(61));
    let req = rig.request("scout", &["api"], MessageType::Finding, "finding 6");
    assert!(send(&mut rig.ctx(), req).is_ok());
}

#[test]
fn an_identical_text_to_the_same_recipient_is_a_repeat_for_ten_minutes() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, "the same words");
    assert!(send(&mut rig.ctx(), req.clone()).is_ok());
    assert_eq!(
        refused(send(&mut rig.ctx(), req.clone())),
        Some(Refusal::Repeat)
    );
    // The same words to someone else are a different message.
    let other = rig.request("scout", &["web"], MessageType::Finding, "the same words");
    assert!(send(&mut rig.ctx(), other).is_ok());
    rig.clock.advance(Duration::from_secs(601));
    assert!(send(&mut rig.ctx(), req).is_ok());
}

// ---- the outbox order and delivery states ----

#[test]
fn a_send_is_stored_typed_and_recorded() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, "the answer is 42");
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(report.display_id.as_str(), "m001");
    assert_eq!(report.seq, 1);
    assert!(!report.duplicate_of_key);
    assert_eq!(report.deliveries.len(), 1);
    assert_eq!(only(&report).state, RecipientState::HandedOff);
    assert_eq!(only(&report).attempts, Some(1));
    // The line typed to the recipient carries the short body whole, with no
    // pointer to the rest.
    let typed: Vec<String> = all_submitted(&rig);
    assert_eq!(typed.len(), 1);
    let line = typed.first().unwrap();
    assert!(
        line.starts_with("[teamwire:alpha msg m001 finding from scout] the answer is 42. This is"),
        "{line}"
    );
    // A second message gets the next display id and the sender's next seq.
    let req = rig.request("scout", &["api"], MessageType::Finding, "and another");
    let second = send(&mut rig.ctx(), req).unwrap();
    assert_eq!((second.display_id.as_str(), second.seq), ("m002", 2));
}

fn all_submitted(rig: &Rig) -> Vec<String> {
    rig.host
        .log()
        .calls()
        .iter()
        .filter(|c| c.name == "submit")
        .filter_map(|c| c.detail.split_once(' ').map(|(_, line)| line.to_owned()))
        .collect()
}

#[test]
fn the_comms_copy_holds_the_exact_body() {
    let mut rig = team_rig();
    let body = "line one\nline two — with a dash";
    let req = rig.request("scout", &["api"], MessageType::Finding, body);
    let report = send(&mut rig.ctx(), req).unwrap();
    let path = rig
        .home
        .path()
        .join("teams/alpha/comms")
        .join(format!("{}.md", report.display_id));
    assert_eq!(std::fs::read_to_string(path).unwrap(), body);
}

#[test]
fn a_comms_write_failure_does_not_lose_the_message_or_stop_delivery() {
    let mut rig = team_rig();
    rig.platform
        .fail_writes(Some(teamwire_core::PlatformError::new(
            teamwire_core::PlatformErrorCode::NoSpace,
            "disk full",
        )));
    let req = rig.request("scout", &["api"], MessageType::Finding, "still sent");
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(only(&report).state, RecipientState::HandedOff);
}

#[test]
fn a_comms_copy_that_was_not_written_is_named_as_such_in_the_lead_line() {
    let mut rig = team_rig();
    rig.platform
        .fail_writes(Some(teamwire_core::PlatformError::new(
            teamwire_core::PlatformErrorCode::NoSpace,
            "disk full",
        )));
    let lead_place = teamwire_core::Place {
        host: teamwire_core::HostId::from_static("fake"),
        id: "p1".to_owned(),
    };
    let req = rig.request(
        "web",
        &["lead"],
        MessageType::Finding,
        &format!("the finding {}", "and more ".repeat(40)),
    );
    send(&mut rig.ctx(), req).unwrap();
    let typed = rig.host.submitted(&lead_place);
    let line = typed.last().expect("a line typed into the lead's place");
    assert!(
        line.contains("comms/m001.md (not written; run teamwire verify alpha --repair-files)"),
        "{line}"
    );
}

#[test]
fn replaced_recipient_is_not_delivered() {
    let mut rig = team_rig();
    // The recipient's session is gone: the locator finds no live process.
    rig.set_reachable("api", false);
    let req = rig.request("scout", &["api", "web"], MessageType::Finding, "x");
    let report = send(&mut rig.ctx(), req).unwrap();
    let by_role = |name: &str| report.deliveries.iter().find(|d| d.to == to(name)).unwrap();
    assert_eq!(by_role("api").state, RecipientState::Undelivered);
    assert_eq!(
        by_role("api").reason,
        Some(DeliveryErrorCode::NoLiveProcess)
    );
    // The other recipient has its own state.
    assert_eq!(by_role("web").state, RecipientState::HandedOff);
    // The message stays in the store with the reason (MSG-12).
    let row = rig
        .store
        .read(|tx| tx.delivery(&report.id, &to("api")))
        .unwrap()
        .unwrap();
    assert_eq!(
        row.delivery.error.unwrap().code,
        DeliveryErrorCode::NoLiveProcess
    );
}

#[test]
fn a_stalled_prompt_is_unconfirmed_and_typed_once() {
    let mut rig = team_rig();
    rig.host.inject(
        "submit",
        Fault::Fail(HostError::new(HostErrorCode::Stalled, "stalled")),
        5,
    );
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(only(&report).state, RecipientState::Unconfirmed);
    assert_eq!(only(&report).reason, Some(DeliveryErrorCode::HostStalled));
    assert_eq!(all_submitted(&rig).len(), 1);
}

#[test]
fn resume_with_key_delivers_pending_and_undelivered_only() {
    let mut rig = team_rig();
    rig.add_member("docs", MemberKind::Implementer);
    // A first try that crashed after the commit: three rows, one already
    // handed off, one undelivered, one still pending.
    let at = rig.now();
    let new = teamwire_store::NewMessage {
        sender: teamwire_core::Actor::parse("scout").unwrap(),
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: "the finding".to_owned(),
        client_key: Some("k1".to_owned()),
        recipients: vec![to("api"), to("web"), to("docs")],
    };
    let stored = rig.store.write(|tx| tx.insert_message(at, &new)).unwrap();
    rig.store
        .write(|tx| {
            tx.set_delivery_state(
                at,
                &stored.message.ulid,
                &to("api"),
                &teamwire_store::DeliveryUpdate {
                    state: teamwire_core::DeliveryState::HandedOff,
                    route: Some(teamwire_core::Route::HostTyping),
                    error: None,
                    attempts: 1,
                },
            )?;
            tx.set_delivery_state(
                at,
                &stored.message.ulid,
                &to("web"),
                &teamwire_store::DeliveryUpdate {
                    state: teamwire_core::DeliveryState::Undelivered,
                    route: None,
                    error: Some(teamwire_core::DeliveryError::new(
                        DeliveryErrorCode::HostListingFailed,
                        "x",
                    )),
                    attempts: 1,
                },
            )
        })
        .unwrap();
    // The pending row's attempt is over a minute old, so no sender is
    // still delivering it.
    rig.clock.advance(Duration::from_secs(61));
    let mut req = rig.request(
        "scout",
        &["api", "web", "docs"],
        MessageType::Finding,
        "the finding",
    );
    req.client_key = Some("k1".to_owned());
    let seen = RefCell::new(None);
    let report = send_with(&mut rig.ctx(), req, &mut |s| {
        *seen.borrow_mut() = Some(s.clone());
    })
    .unwrap();
    assert!(report.duplicate_of_key);
    assert_eq!(report.id, stored.message.ulid);
    let state = |name: &str| report.deliveries.iter().find(|d| d.to == to(name)).unwrap();
    assert_eq!(state("api").state, RecipientState::HandedOff);
    assert_eq!(state("api").attempts, Some(1));
    assert_eq!(state("web").state, RecipientState::HandedOff);
    assert_eq!(state("web").attempts, Some(2));
    assert_eq!(state("docs").state, RecipientState::HandedOff);
    // api was not typed to again; web and docs were, once each.
    assert_eq!(all_submitted(&rig).len(), 2);
    let seen = seen.borrow().clone().unwrap();
    assert!(seen.duplicate);
    assert_eq!(seen.key, "k1");
    // No second message, no second seq.
    assert_eq!(
        rig.store.read(|tx| tx.counter("display_id")).unwrap(),
        Some(1)
    );
}

/// A first send with key `k1` to `api` that is still delivering: its row is
/// committed `pending` and nothing has been typed yet.
fn still_delivering(rig: &mut Rig) -> teamwire_store::StoredMessage {
    let at = rig.now();
    let new = teamwire_store::NewMessage {
        sender: teamwire_core::Actor::parse("scout").unwrap(),
        kind: MessageType::Finding,
        re: None,
        commit: None,
        body: "the finding".to_owned(),
        client_key: Some("k1".to_owned()),
        recipients: vec![to("api")],
    };
    rig.store.write(|tx| tx.insert_message(at, &new)).unwrap()
}

fn resume_k1(rig: &mut Rig) -> SendReport {
    let mut req = rig.request("scout", &["api"], MessageType::Finding, "the finding");
    req.client_key = Some("k1".to_owned());
    send(&mut rig.ctx(), req).unwrap()
}

#[test]
fn a_resume_within_a_minute_of_the_attempt_skips_the_row() {
    let mut rig = team_rig();
    still_delivering(&mut rig);
    rig.clock.advance(Duration::from_secs(59));
    let report = resume_k1(&mut rig);
    assert!(report.duplicate_of_key);
    assert_eq!(only(&report).state, RecipientState::Skipped);
    assert!(all_submitted(&rig).is_empty());
}

#[test]
fn a_resume_claims_the_row_so_a_second_resume_skips_it() {
    let mut rig = team_rig();
    let stored = still_delivering(&mut rig);
    rig.clock.advance(Duration::from_secs(61));
    // The claim restarts the attempt clock, so a second resume that reads
    // the row before this one records its outcome finds it busy.
    let at = rig.now();
    let attempts = rig
        .store
        .write(|tx| tx.begin_resend(at, &stored.message.ulid, &to("api")))
        .unwrap();
    assert_eq!(attempts, 1);
    let report = resume_k1(&mut rig);
    assert_eq!(only(&report).state, RecipientState::Skipped);
    assert!(all_submitted(&rig).is_empty());
    rig.clock.advance(Duration::from_secs(61));
    let report = resume_k1(&mut rig);
    assert_eq!(only(&report).state, RecipientState::HandedOff);
    assert_eq!(only(&report).attempts, Some(2));
    assert_eq!(all_submitted(&rig).len(), 1);
    let resent = rig
        .store
        .read(|tx| {
            tx.events_of_type(teamwire_core::EventType::DeliveryResent)?
                .all_or_corrupt()
        })
        .unwrap();
    assert_eq!(resent.len(), 2);
}

#[test]
fn a_resume_skips_an_undelivered_row_another_command_just_claimed() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let mut req = rig.request("scout", &["api"], MessageType::Finding, "the finding");
    req.client_key = Some("k1".to_owned());
    let first = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(only(&first).state, RecipientState::Undelivered);
    rig.set_reachable("api", true);
    // Another resume claimed the row and is still typing: its claim is on
    // record, its outcome is not.
    let at = rig.now();
    rig.store
        .write(|tx| tx.begin_resend(at, &first.id, &to("api")))
        .unwrap();
    rig.clock.advance(Duration::from_secs(2));
    let report = resume_k1(&mut rig);
    assert_eq!(only(&report).state, RecipientState::Skipped);
    assert!(all_submitted(&rig).is_empty());
    // A minute on, that command is taken for dead and the row is retried.
    rig.clock.advance(Duration::from_secs(61));
    let report = resume_k1(&mut rig);
    assert_eq!(only(&report).state, RecipientState::HandedOff);
    assert_eq!(all_submitted(&rig).len(), 1);
}

#[test]
fn a_resume_retries_an_undelivered_row_whose_last_attempt_ended() {
    let mut rig = team_rig();
    rig.set_reachable("api", false);
    let mut req = rig.request("scout", &["api"], MessageType::Finding, "the finding");
    req.client_key = Some("k1".to_owned());
    send(&mut rig.ctx(), req).unwrap();
    rig.set_reachable("api", true);
    let report = resume_k1(&mut rig);
    assert_eq!(only(&report).state, RecipientState::HandedOff);
    assert_eq!(all_submitted(&rig).len(), 1);
}

#[test]
fn the_key_is_reported_before_any_host_call() {
    let mut rig = team_rig();
    let req = rig.request("scout", &["api"], MessageType::Finding, "x");
    let before = RefCell::new(None);
    let baseline = rig.host.log().calls().len();
    let host = rig.host.clone();
    let report = send_with(&mut rig.ctx(), req, &mut |s| {
        *before.borrow_mut() = Some((
            s.display_id.clone(),
            s.key.clone(),
            host.log().calls().len(),
        ));
    })
    .unwrap();
    let (display, key, calls) = before.borrow().clone().unwrap();
    assert_eq!(display, report.display_id);
    // With no key given, the key is the message's own ULID.
    assert_eq!(key, report.id.to_string());
    assert_eq!(
        calls, baseline,
        "a host call happened before the id was reported"
    );
    assert!(rig.host.log().calls().len() > baseline);
}

/// A clock that, each time delivery reads it, tries a write from a second
/// connection. Any write lock held by the sender would make that fail.
struct ProbeClock {
    inner: teamwire_fakes::FakeClock,
    probe: Mutex<Store>,
    outcomes: Mutex<Vec<bool>>,
}

impl Clock for ProbeClock {
    fn now(&self) -> Timestamp {
        self.inner.now()
    }

    fn monotonic(&self) -> Instant {
        let at = self.inner.now();
        let wrote = self
            .probe
            .lock()
            .unwrap()
            .write(|tx| {
                tx.record_event(at, EventType::WatchLine, None, None, &json!({}))
                    .map(|_| ())
            })
            .is_ok();
        self.outcomes.lock().unwrap().push(wrote);
        self.inner.monotonic()
    }
}

#[test]
fn no_write_lock_held_during_host_call() {
    let mut rig = team_rig();
    let team = teamwire_core::TeamName::parse("alpha").unwrap();
    let clock = ProbeClock {
        inner: teamwire_fakes::FakeClock::new(),
        probe: Mutex::new(Store::open_existing(rig.home.path(), &team).unwrap()),
        outcomes: Mutex::new(Vec::new()),
    };
    let req = rig.request("scout", &["api", "web"], MessageType::Finding, "x");
    let mut ctx = Ctx {
        home: rig.home.path(),
        store: &mut rig.store,
        registry: &rig.registry,
        platform: &rig.platform,
        clock: &clock,
        config: &rig.config,
    };
    let report = send(&mut ctx, req).unwrap();
    assert!(
        report
            .deliveries
            .iter()
            .all(|d| d.state == RecipientState::HandedOff)
    );
    let outcomes = clock.outcomes.lock().unwrap();
    assert!(outcomes.len() >= 2, "the probe never ran during delivery");
    assert!(
        outcomes.iter().all(|ok| *ok),
        "another writer was blocked during delivery: {outcomes:?}"
    );
}

#[test]
fn a_question_to_the_lead_tells_the_lead_how_to_answer_and_where_the_text_is() {
    let mut rig = team_rig();
    // Rig::new makes the lead's place first, and the fake host numbers
    // places from p1.
    let lead_place = teamwire_core::Place {
        host: teamwire_core::HostId::from_static("fake"),
        id: "p1".to_owned(),
    };
    let req = rig.request(
        "web",
        &["lead"],
        MessageType::Question,
        // Past the excerpt, so the line points at the stored copy.
        &format!("which schema version? {}", "and more ".repeat(40)),
    );
    send(&mut rig.ctx(), req).unwrap();
    let typed = rig.host.submitted(&lead_place);
    let line = typed.last().expect("a line typed into the lead's place");
    assert!(
        line.contains("--from lead --to web --type answer --re m"),
        "{line}"
    );
    assert!(line.contains("/comms/m"), "{line}");
    assert!(!line.contains("teamwire inbox"), "{line}");
}

#[test]
fn a_failed_record_for_one_recipient_still_delivers_to_the_rest() {
    let mut rig = team_rig();
    // Recording the outcome for `api` fails; `web` is untouched.
    rig.store
        .write(|tx| {
            tx.connection()
                .execute_batch(
                    "CREATE TRIGGER no_api BEFORE UPDATE ON deliveries
                     WHEN NEW.recipient = 'api' BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
                )
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let req = rig.request(
        "scout",
        &["api", "web"],
        MessageType::Finding,
        "the finding",
    );
    let report = send(&mut rig.ctx(), req).unwrap();
    let state = |name: &str| {
        report
            .deliveries
            .iter()
            .find(|d| d.to == to(name))
            .unwrap()
            .state
    };
    assert_eq!(state("api"), RecipientState::Pending);
    assert_eq!(state("web"), RecipientState::HandedOff);
}

#[test]
fn crlf_and_bom_do_not_count_toward_the_limit() {
    let mut rig = team_rig();
    // 16 lines of 790 characters with plain line ends; a Windows file adds
    // a byte-order mark and a carriage return per line end.
    let lines: Vec<String> = (0..16)
        .map(|i| "x".repeat(if i < 15 { 48 } else { 55 }))
        .collect();
    let plain = lines.join("\n");
    assert_eq!(plain.chars().count(), 790);
    let windows = format!("\u{feff}{}", lines.join("\r\n"));
    assert_eq!(windows.chars().count(), 806);
    let req = rig.request("scout", &["api"], MessageType::Finding, &windows);
    let report = send(&mut rig.ctx(), req).unwrap();
    assert_eq!(only(&report).state, RecipientState::HandedOff);
    let stored = rig
        .store
        .read(|tx| tx.message_by_display_id(&report.display_id))
        .unwrap()
        .unwrap();
    assert_eq!(stored.body, plain);
}

// ---- a store write inside the message's transaction (teamwire amend) ----

fn lead_note(rig: &Rig, body: &str) -> SendRequest {
    let req = rig.request("api", &["api"], MessageType::Note, body);
    SendRequest {
        actor: teamwire_core::Actor::parse("lead").unwrap(),
        ..req
    }
}

fn scope_of(rig: &Rig, name: &str) -> (Option<String>, Vec<String>) {
    let m = rig
        .store
        .read(|tx| tx.member(&role(name)))
        .unwrap()
        .unwrap();
    (m.check, m.owns)
}

fn delivery_rows(rig: &Rig) -> usize {
    rig.store
        .read(|tx| {
            let mut n = 0;
            for m in tx
                .messages(&teamwire_store::MessageFilter::default())?
                .items
            {
                n += tx.deliveries_of(&m.ulid)?.all_or_corrupt()?.len();
            }
            Ok(n)
        })
        .unwrap()
}

#[test]
fn send_with_write_stores_the_message_and_the_write_together() {
    // (a) The write and the message are both visible after the call.
    let mut rig = team_rig();
    let before = scope_of(&rig, "api");
    let new_owns = vec!["b/".to_owned()];
    let req = lead_note(&rig, "CHECK AMENDED: why");
    let report = send_with_write(
        &mut rig.ctx(),
        req,
        &|tx| tx.set_member_scope(&role("api"), Some("cargo test -p b"), &new_owns),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        scope_of(&rig, "api"),
        (Some("cargo test -p b".to_owned()), new_owns.clone()),
        "the scope before was {before:?}"
    );
    assert_eq!(report.deliveries.len(), 1);
    assert!(!nothing_stored(&rig));
    assert_eq!(delivery_rows(&rig), 1);

    // (b) A write that fails rolls the message back with it.
    let mut rig = team_rig();
    let before = scope_of(&rig, "api");
    let req = lead_note(&rig, "CHECK AMENDED: why");
    let result = send_with_write(
        &mut rig.ctx(),
        req,
        &|tx| {
            tx.set_member_scope(&role("api"), Some("cargo test -p b"), &new_owns)?;
            Err(teamwire_store::StoreError::Constraint("boom".to_owned()))
        },
        &mut |_| {},
    );
    assert!(
        matches!(&result, Err(SendError::Store(teamwire_store::StoreError::Constraint(m))) if m == "boom"),
        "{result:?}"
    );
    assert!(nothing_stored(&rig));
    assert_eq!(delivery_rows(&rig), 0);
    assert_eq!(scope_of(&rig, "api"), before);

    // (c) A team that is not open runs no write and refuses as `send` does.
    let mut rig = team_rig();
    let now = rig.now();
    rig.store
        .write(|tx| tx.begin_close(now, Pid::new(4242)).map(|_| ()))
        .unwrap();
    let ran = std::cell::Cell::new(false);
    let req = lead_note(&rig, "CHECK AMENDED: why");
    let result = send_with_write(
        &mut rig.ctx(),
        req,
        &|_| {
            ran.set(true);
            Ok(())
        },
        &mut |_| {},
    );
    assert_eq!(refused(result), Some(Refusal::TeamClosed));
    assert!(!ran.get());
    assert!(nothing_stored(&rig));
}

/// A clock that stands for a verify that ran the member's old check and
/// waits for the store lock. At each reading it moves time on one second
/// and tries to commit that verify's PASS from a second connection, which
/// gives up at once when another writer holds the lock. It returns the time
/// from before the move, as a command that read the clock first and wrote
/// later would hold it. The rival stops once the message is stored.
struct RivalPassAtEachRead {
    inner: teamwire_fakes::FakeClock,
    rival: Mutex<Store>,
    armed: std::sync::atomic::AtomicBool,
    /// Each reading's time, and whether the rival's PASS committed then.
    reads: Mutex<Vec<(Timestamp, bool)>>,
}

impl Clock for RivalPassAtEachRead {
    fn now(&self) -> Timestamp {
        let read = self.inner.now();
        if !self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            return read;
        }
        self.inner.advance(Duration::from_secs(1));
        let at = self.inner.now();
        let pass = teamwire_store::NewVerifyRecord {
            role: role("api"),
            head_sha: FULL_SHA.to_owned(),
            verdict: "PASS".to_owned(),
            exit_code: Some(0),
            seconds: Some(1),
            flags: Vec::new(),
            log_sha256: None,
        };
        let committed = self
            .rival
            .lock()
            .unwrap()
            .write(|tx| tx.insert_verify_record(at, &pass).map(|_| ()))
            .is_ok();
        self.reads.lock().unwrap().push((read, committed));
        read
    }

    fn monotonic(&self) -> Instant {
        self.inner.monotonic()
    }
}

/// `close --role` counts a PASS only when its time is after the newest amend
/// note's (ADR 0046). So a PASS made with the old check that commits before
/// the note must carry a time at or before the note's, whatever order the
/// two commands read the clock in. The note's time is read inside the write
/// that stores it, under the store lock.
#[test]
fn a_pass_committed_before_the_amend_note_is_not_timed_after_it() {
    let mut rig = team_rig();
    let team = teamwire_core::TeamName::parse("alpha").unwrap();
    let rival = Store::open_existing(rig.home.path(), &team).unwrap();
    rival
        .read(|tx| {
            tx.connection().busy_timeout(Duration::ZERO)?;
            Ok(())
        })
        .unwrap();
    let clock = RivalPassAtEachRead {
        inner: teamwire_fakes::FakeClock::new(),
        rival: Mutex::new(rival),
        armed: std::sync::atomic::AtomicBool::new(true),
        reads: Mutex::new(Vec::new()),
    };
    let req = lead_note(&rig, "CHECK AMENDED: why");
    let owns = vec!["b/".to_owned()];
    let mut ctx = Ctx {
        home: rig.home.path(),
        store: &mut rig.store,
        registry: &rig.registry,
        platform: &rig.platform,
        clock: &clock,
        config: &rig.config,
    };
    let report = send_with_write(
        &mut ctx,
        req,
        &|tx| tx.set_member_scope(&role("api"), Some("cargo test -p b"), &owns),
        &mut |_| {
            clock
                .armed
                .store(false, std::sync::atomic::Ordering::SeqCst);
        },
    )
    .unwrap();
    let note = rig
        .store
        .read(|tx| tx.message_by_display_id(&report.display_id))
        .unwrap()
        .unwrap();
    // Every PASS stored now committed during a reading before the note was
    // stored.
    let passes = rig
        .store
        .read(|tx| tx.verify_records(&role("api")))
        .unwrap()
        .all_or_corrupt()
        .unwrap();
    let reads = clock.reads.lock().unwrap().clone();
    for pass in &passes {
        assert!(
            pass.recorded_at <= note.created_at,
            "a PASS committed before the note at {:?} carries a later time than the note's {:?}; \
             close --role would count it (reads: {reads:?})",
            pass.recorded_at,
            note.created_at
        );
    }
    let at_note: Vec<bool> = reads
        .iter()
        .filter(|(t, _)| *t == note.created_at)
        .map(|(_, committed)| *committed)
        .collect();
    assert_eq!(
        at_note,
        [false],
        "the note's time was not read under the store lock: {reads:?}"
    );
}
