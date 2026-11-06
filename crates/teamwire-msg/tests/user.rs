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
