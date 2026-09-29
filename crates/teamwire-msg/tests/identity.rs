//! Who is speaking (ADR 0008): a member by its token, the lead by its
//! harness session. Anything else is `refused.identity`.
#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use serde_json::json;
use teamwire_core::{Actor, EventType, MemberKind, Refusal};
use teamwire_msg::{CallerEnv, prove_actor};

use rig::{Rig, role, sid, token_of};

fn member_env(name: &str, token: &str) -> CallerEnv {
    CallerEnv {
        team: Some("alpha".to_owned()),
        role: Some(name.to_owned()),
        token: Some(token.to_owned()),
        from: None,
    }
}

fn lead_env() -> CallerEnv {
    CallerEnv {
        from: Some("lead".to_owned()),
        ..CallerEnv::default()
    }
}

fn rig_with_members() -> Rig {
    let mut rig = Rig::new();
    rig.add_member("api", MemberKind::Implementer);
    rig.add_member("web", MemberKind::Implementer);
    rig
}

#[test]
fn a_member_with_its_token_is_proven() {
    let mut rig = rig_with_members();
    let env = member_env("api", &token_of("api"));
    assert_eq!(
        prove_actor(&rig.ctx(), &env),
        Ok(Actor::Member(role("api")))
    );
    // `--from` naming the same role is the same claim.
    let env = CallerEnv {
        from: Some("api".to_owned()),
        ..env
    };
    assert_eq!(
        prove_actor(&rig.ctx(), &env),
        Ok(Actor::Member(role("api")))
    );
}

#[test]
fn wrong_token_refused() {
    let mut rig = rig_with_members();
    // Another member's token, a made-up token, and no token at all.
    for token in [Some(token_of("web")), Some("guess".to_owned()), None] {
        let env = CallerEnv {
            team: Some("alpha".to_owned()),
            role: Some("api".to_owned()),
            token,
            from: None,
        };
        assert_eq!(prove_actor(&rig.ctx(), &env), Err(Refusal::Identity));
    }
}

#[test]
fn a_member_cannot_speak_as_another_member_or_the_lead() {
    let mut rig = rig_with_members();
    for from in ["web", "lead"] {
        let env = CallerEnv {
            from: Some(from.to_owned()),
            ..member_env("api", &token_of("api"))
        };
        assert_eq!(
            prove_actor(&rig.ctx(), &env),
            Err(Refusal::Identity),
            "{from}"
        );
    }
}

#[test]
fn a_token_for_another_team_or_a_removed_member_is_refused() {
    let mut rig = rig_with_members();
    let env = CallerEnv {
        team: Some("beta".to_owned()),
        ..member_env("api", &token_of("api"))
    };
    assert_eq!(prove_actor(&rig.ctx(), &env), Err(Refusal::Identity));
    let now = rig.now();
    rig.store
        .write(|tx| {
            tx.remove_member(now, &role("api"), None, &json!({}))
                .map(|_| ())
        })
        .unwrap();
    let env = member_env("api", &token_of("api"));
    assert_eq!(prove_actor(&rig.ctx(), &env), Err(Refusal::Identity));
}

#[test]
fn the_lead_is_proven_by_its_session() {
    let mut rig = rig_with_members();
    assert_eq!(prove_actor(&rig.ctx(), &lead_env()), Ok(Actor::Lead));
}

#[test]
fn lead_without_session_refused() {
    let mut rig = rig_with_members();
    // The harness reports no session id: nothing can be proven (DEL-10).
    rig.harness.set_caller(None);
    assert_eq!(prove_actor(&rig.ctx(), &lead_env()), Err(Refusal::Identity));
    // A session that is not the team's lead session.
    rig.harness
        .set_interactive_caller(sid("some-other-session"));
    assert_eq!(prove_actor(&rig.ctx(), &lead_env()), Err(Refusal::Identity));
    // A background session with the right id.
    rig.harness.set_background_caller(sid("lead-1"));
    assert_eq!(prove_actor(&rig.ctx(), &lead_env()), Err(Refusal::Identity));
}

#[test]
fn the_lead_claim_needs_from_lead_and_no_member_identity() {
    let mut rig = rig_with_members();
    // No `--from lead`.
    assert_eq!(
        prove_actor(&rig.ctx(), &CallerEnv::default()),
        Err(Refusal::Identity)
    );
    // A member's environment without its token, even in the lead's session.
    let env = CallerEnv {
        role: Some("api".to_owned()),
        ..lead_env()
    };
    assert_eq!(prove_actor(&rig.ctx(), &env), Err(Refusal::Identity));
    // A token with `--from lead` was tested above: the token wins and refuses.
}

#[test]
fn a_refused_identity_is_logged_and_stores_no_message() {
    let mut rig = rig_with_members();
    rig.harness.set_caller(None);
    let refusal = prove_actor(&rig.ctx(), &lead_env()).unwrap_err();
    teamwire_msg::record_refusal(&mut rig.ctx(), None, &refusal, None, &[]);
    let events = rig.events(EventType::MessageRefused);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events.first().and_then(|e| e.data.get("code")),
        Some(&json!("identity"))
    );
    assert!(rig.events(EventType::MessageSent).is_empty());
}

#[test]
fn a_library_level_identity_refusal_writes_the_event_with_code_and_reason() {
    let mut rig = rig_with_members();
    let env = member_env("api", "wrong-token");
    let refusal = teamwire_msg::prove_actor_logged(&mut rig.ctx(), &env).unwrap_err();
    assert_eq!(refusal, Refusal::Identity);
    let events = rig.events(EventType::MessageRefused);
    assert_eq!(events.len(), 1);
    let data = &events.first().unwrap().data;
    assert_eq!(data.get("code"), Some(&json!("identity")));
    assert_eq!(data.get("reason"), Some(&json!("identity")));
    // A proven caller writes nothing.
    let ok = member_env("api", &token_of("api"));
    assert!(teamwire_msg::prove_actor_logged(&mut rig.ctx(), &ok).is_ok());
    assert_eq!(rig.events(EventType::MessageRefused).len(), 1);
}

#[test]
fn an_unknown_type_is_refused_and_logged_by_the_library() {
    let mut rig = rig_with_members();
    let actor = Actor::Member(role("api"));
    let to = [rig::to("web")];
    let parsed = teamwire_msg::parse_type_logged(&mut rig.ctx(), &actor, "gossip", &to);
    assert_eq!(parsed, Err(Refusal::TypeNotAllowed));
    let events = rig.events(EventType::MessageRefused);
    assert_eq!(events.len(), 1);
    let data = &events.first().unwrap().data;
    assert_eq!(data.get("reason"), Some(&json!("type_not_allowed")));
    assert_eq!(data.get("sender"), Some(&json!("api")));
    let known = teamwire_msg::parse_type_logged(&mut rig.ctx(), &actor, "finding", &to);
    assert_eq!(known, Ok(teamwire_core::MessageType::Finding));
    assert_eq!(rig.events(EventType::MessageRefused).len(), 1);
}
