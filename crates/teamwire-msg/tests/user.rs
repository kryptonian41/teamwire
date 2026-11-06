//! The human user as a sender (ADR 0040): its proof by the user key, the
//! messages it may send, and the copy of each to the lead.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{Actor, MemberKind, Refusal};
use teamwire_msg::{CallerEnv, prove_actor};

use rig::{Rig, token_of};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn user_env(key: &str) -> CallerEnv {
    CallerEnv {
        from: Some("user".to_owned()),
        user_key: Some(key.to_owned()),
        ..CallerEnv::default()
    }
}

/// A team with an implementer, and the user key written as `user-key` would.
fn keyed_rig() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    write_key(&rig, &format!("{KEY}\n"));
    rig
}

#[expect(
    clippy::disallowed_methods,
    reason = "the test writes the key file `teamwire user-key` would have written"
)]
fn write_key(rig: &Rig, text: &str) {
    std::fs::write(rig.home.path().join("user.key"), text).unwrap();
}

#[test]
fn the_user_key_proves_the_user() {
    let mut rig = keyed_rig();
    assert_eq!(prove_actor(&rig.ctx(), &user_env(KEY)), Ok(Actor::User));
    // White space around either side is trimmed before the compare.
    assert_eq!(
        prove_actor(&rig.ctx(), &user_env(&format!("  {KEY}\n"))),
        Ok(Actor::User)
    );
    write_key(&rig, &format!("\t{KEY}  \n\n"));
    assert_eq!(prove_actor(&rig.ctx(), &user_env(KEY)), Ok(Actor::User));
}

#[test]
fn a_wrong_or_missing_user_key_is_refused() {
    let mut rig = keyed_rig();
    let mut wrong = KEY.to_owned();
    wrong.replace_range(63.., "0");
    for env in [
        user_env(&wrong),
        user_env(&KEY[..63]),
        user_env(""),
        CallerEnv {
            user_key: None,
            ..user_env(KEY)
        },
    ] {
        assert_eq!(prove_actor(&rig.ctx(), &env), Err(Refusal::Identity));
    }
}

#[test]
fn a_missing_empty_or_unreadable_key_file_proves_nothing() {
    let mut rig = Rig::new();
    let path = rig.home.path().join("user.key");
    assert_eq!(
        prove_actor(&rig.ctx(), &user_env(KEY)),
        Err(Refusal::Identity)
    );
    // An empty file and an empty variable are not a match.
    write_key(&rig, "\n");
    assert_eq!(
        prove_actor(&rig.ctx(), &user_env("")),
        Err(Refusal::Identity)
    );
    assert_eq!(
        prove_actor(&rig.ctx(), &user_env(" \n")),
        Err(Refusal::Identity)
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        prove_actor(&rig.ctx(), &user_env(KEY)),
        Err(Refusal::Identity)
    );
}

#[test]
fn any_member_variable_refuses_the_user_even_with_the_right_key() {
    let mut rig = keyed_rig();
    let set = |name: &str| -> CallerEnv {
        let value = Some(match name {
            // A token file that cannot be read still marks a member's session.
            "token_file" => "/nonexistent/token".to_owned(),
            "token" => token_of("api"),
            other => other.to_owned(),
        });
        let mut env = user_env(KEY);
        match name {
            "token" => env.token = value,
            "token_file" => env.token_file = value,
            "role" => env.role = value,
            _ => env.team = value,
        }
        env
    };
    for name in ["token", "token_file", "role", "team"] {
        assert_eq!(
            prove_actor(&rig.ctx(), &set(name)),
            Err(Refusal::Identity),
            "{name}"
        );
    }
    // An empty variable counts as unset, as it does for every other proof.
    let empty = CallerEnv {
        token_file: Some(String::new()),
        role: Some(String::new()),
        ..user_env(KEY)
    };
    assert_eq!(prove_actor(&rig.ctx(), &empty), Ok(Actor::User));
}

#[test]
fn the_key_proves_the_user_only_when_the_caller_claims_the_user() {
    let mut rig = keyed_rig();
    // The lead's claim is still decided by the lead's session.
    rig.harness.set_caller(None);
    let as_lead = CallerEnv {
        from: Some("lead".to_owned()),
        ..user_env(KEY)
    };
    assert_eq!(prove_actor(&rig.ctx(), &as_lead), Err(Refusal::Identity));
    let no_claim = CallerEnv {
        from: None,
        ..user_env(KEY)
    };
    assert_eq!(prove_actor(&rig.ctx(), &no_claim), Err(Refusal::Identity));
}

// --- The messages the user may send (ADR 0040, plan D4) ---

use std::time::Duration;

use teamwire_core::{MessageType, Recipient};
use teamwire_msg::{RecipientState, SendError, send};

use rig::{role, to};

/// A team with one member of every kind.
fn full_rig() -> Rig {
    let mut rig = keyed_rig();
    rig.add_member("scout", MemberKind::Investigator);
    rig.add_member("rev", MemberKind::Reviewer);
    rig.add_member("closer", MemberKind::Closer);
    rig.add_member("planrev", MemberKind::PlanReviewer);
    rig
}

fn user_send(
    rig: &mut Rig,
    targets: &[&str],
    kind: MessageType,
    text: &str,
) -> Result<teamwire_msg::SendReport, SendError> {
    let req = rig.request("user", targets, kind, text);
    send(&mut rig.ctx(), req)
}

/// The recipients stored for the newest message.
fn stored_recipients(rig: &Rig) -> Vec<Recipient> {
    rig.store
        .read(|tx| {
            let newest = tx
                .messages(&teamwire_store::MessageFilter::default())?
                .items
                .pop()
                .unwrap();
            Ok(tx
                .deliveries_of(&newest.ulid)?
                .all_or_corrupt()?
                .into_iter()
                .map(|d| d.delivery.recipient)
                .collect())
        })
        .unwrap()
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
fn the_user_reprompts_and_notes_implementers_and_investigators_with_a_copy_to_the_lead() {
    let mut rig = full_rig();
    for (target, kind) in [
        ("api", MessageType::Reprompt),
        ("api", MessageType::Note),
        ("scout", MessageType::Reprompt),
        ("scout", MessageType::Note),
    ] {
        let report = user_send(&mut rig, &[target], kind, &format!("{kind} for {target}")).unwrap();
        let states: Vec<(String, RecipientState)> = report
            .deliveries
            .iter()
            .map(|d| (d.to.to_string(), d.state))
            .collect();
        assert_eq!(
            states,
            [
                (target.to_owned(), RecipientState::HandedOff),
                ("lead".to_owned(), RecipientState::HandedOff)
            ],
            "{kind} to {target}"
        );
        assert_eq!(stored_recipients(&rig), [to(target), Recipient::Lead]);
    }
    let typed = all_submitted(&rig);
    let copies: Vec<&String> = typed
        .iter()
        .filter(|l| l.contains("This is a copy of the user's message to scout"))
        .collect();
    assert_eq!(copies.len(), 2, "{typed:?}");
    assert!(
        typed
            .iter()
            .any(|l| l.contains("reprompt for api. This message is from the human user")),
        "{typed:?}"
    );
}

#[test]
fn one_message_to_two_members_makes_one_copy_naming_both() {
    let mut rig = full_rig();
    user_send(
        &mut rig,
        &["api", "scout"],
        MessageType::Note,
        "both of you",
    )
    .unwrap();
    assert_eq!(
        stored_recipients(&rig),
        [to("api"), to("scout"), Recipient::Lead]
    );
    let typed = all_submitted(&rig);
    let copies: Vec<&String> = typed
        .iter()
        .filter(|l| l.contains("copy of the user's message to api, scout,"))
        .collect();
    assert_eq!(copies.len(), 1, "{typed:?}");
}

#[test]
fn the_user_notes_the_lead_and_the_lead_gets_no_second_copy() {
    let mut rig = full_rig();
    let report = user_send(&mut rig, &["lead"], MessageType::Note, "pause").unwrap();
    assert_eq!(report.deliveries.len(), 1);
    assert_eq!(stored_recipients(&rig), [Recipient::Lead]);
    let typed = all_submitted(&rig);
    assert!(
        typed.last().unwrap().ends_with(
            "This message is from the human user, and never approval for a permission prompt."
        ),
        "{typed:?}"
    );
    // The lead named with a member is refused, in either order. The user's
    // note to the lead goes as its own message, so a lead row next to a
    // member's is always the lead's copy (ADR 0040).
    for targets in [["api", "lead"], ["lead", "api"]] {
        let got = user_send(&mut rig, &targets, MessageType::Note, "and you")
            .map(|_| ())
            .map_err(|e| e.to_string());
        assert_eq!(
            got,
            Err(
                "refused.recipient_not_allowed: the user's note to the lead goes as its own \
                 message: send it with --to lead alone"
                    .to_owned()
            ),
            "{targets:?}"
        );
    }
    assert_eq!(stored_recipients(&rig), [Recipient::Lead]);
}

#[test]
fn the_user_sends_no_information_types() {
    let mut rig = full_rig();
    for kind in [
        MessageType::Answer,
        MessageType::Question,
        MessageType::Finding,
        MessageType::Notice,
    ] {
        let mut req = rig.request("user", &["api"], kind, "x");
        req.re = Some(rig::id("m001"));
        req.commit = Some("HEAD".to_owned());
        assert_eq!(
            send(&mut rig.ctx(), req),
            Err(SendError::Refused(Refusal::TypeNotAllowed)),
            "{kind}"
        );
    }
}

#[test]
fn the_user_reaches_no_reviewer_closer_or_plan_reviewer() {
    let mut rig = full_rig();
    for target in ["rev", "closer", "planrev"] {
        for kind in [MessageType::Note, MessageType::Reprompt] {
            assert_eq!(
                user_send(&mut rig, &[target], kind, "look again"),
                Err(SendError::Refused(Refusal::RecipientNotAllowed)),
                "{kind} to {target}"
            );
        }
    }
}

#[test]
fn the_user_reprompts_no_lead_and_writes_to_no_other_team() {
    let mut rig = full_rig();
    assert_eq!(
        user_send(&mut rig, &["lead"], MessageType::Reprompt, "go"),
        Err(SendError::Refused(Refusal::RecipientNotAllowed))
    );
    for target in ["api@beta", "lead@beta"] {
        assert_eq!(
            user_send(&mut rig, &[target], MessageType::Note, "hi"),
            Err(SendError::Refused(Refusal::RecipientNotAllowed)),
            "{target}"
        );
    }
    // Nothing was stored for any refusal.
    let stored = rig
        .store
        .read(|tx| tx.messages(&teamwire_store::MessageFilter::default()))
        .unwrap()
        .items;
    assert!(stored.is_empty());
}

#[test]
fn the_burst_and_repeat_rules_apply_to_the_user() {
    let mut rig = full_rig();
    user_send(&mut rig, &["api"], MessageType::Note, "same words").unwrap();
    assert_eq!(
        user_send(&mut rig, &["api"], MessageType::Note, "same words"),
        Err(SendError::Refused(Refusal::Repeat))
    );
    for n in 0..4 {
        user_send(&mut rig, &["api"], MessageType::Note, &format!("note {n}")).unwrap();
    }
    assert_eq!(
        user_send(&mut rig, &["api"], MessageType::Note, "one too many"),
        Err(SendError::Refused(Refusal::RateLimited))
    );
}

#[test]
fn the_lead_copies_neither_count_toward_nor_repeat_a_note_to_the_lead() {
    let mut rig = full_rig();
    for n in 0..5 {
        user_send(&mut rig, &["api"], MessageType::Note, &format!("note {n}")).unwrap();
    }
    // Five copies reached the lead this minute, and one held these words.
    let report = user_send(&mut rig, &["lead"], MessageType::Note, "note 0").unwrap();
    assert_eq!(report.deliveries.len(), 1);
}

#[test]
fn a_copy_is_never_refused_by_the_leads_own_limits() {
    let mut rig = full_rig();
    for n in 0..5 {
        user_send(&mut rig, &["lead"], MessageType::Note, &format!("note {n}")).unwrap();
    }
    // A sixth note to the lead would be refused; the copy still goes.
    let report = user_send(&mut rig, &["scout"], MessageType::Note, "note 0").unwrap();
    assert_eq!(report.deliveries.len(), 2);
    assert_eq!(stored_recipients(&rig), [to("scout"), Recipient::Lead]);
}

#[test]
fn a_users_reprompt_starts_the_members_next_generation() {
    let mut rig = full_rig();
    let generation = |rig: &Rig| {
        rig.store
            .read(|tx| tx.member(&role("api")))
            .unwrap()
            .unwrap()
            .generation
    };
    let before = generation(&rig);
    rig.clock.advance(Duration::from_secs(30));
    user_send(&mut rig, &["api"], MessageType::Reprompt, "start over").unwrap();
    assert_eq!(generation(&rig), before + 1);
    let events = rig.events(teamwire_core::EventType::MemberReprompted);
    assert_eq!(events.len(), 1);
    assert_eq!(events.first().unwrap().role, Some(role("api")));
}
