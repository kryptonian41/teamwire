//! Proving who is calling (ADR 0008): a member by its token, the lead by its
//! harness session, the human user by the user key (ADR 0040).

use sha2::{Digest as _, Sha256};
use teamwire_core::{Actor, Refusal, Role, SessionKind};
use teamwire_store::{Ctx, Party, user_key_path};

use crate::send::read_team;

use crate::CallerEnv;

/// Proves who is calling.
///
/// A member is accepted only with `TEAMWIRE_TEAM` naming this team and a
/// `TEAMWIRE_TOKEN` whose SHA-256 (of the token's text, as launch hashed it)
/// matches the member's stored hash. `--from`, when given, must name that
/// same role. The lead is accepted only with no token, `--from lead`, and a
/// current harness session that is interactive and equals the team's
/// recorded lead session. The user is accepted only with `--from user`, no
/// member variable set at all, and a `TEAMWIRE_USER_KEY` equal to the key in
/// `<home>/user.key`. Every other case, including a store or key file that
/// cannot be read, is `Identity`: nothing is proven, so nothing is sent
/// (DEL-10).
pub fn prove_actor(ctx: &Ctx<'_>, env: &CallerEnv) -> Result<Actor, Refusal> {
    match (&env.token, env.from.as_deref()) {
        (Some(token), _) => prove_member(ctx, env, token),
        (None, Some("user")) => prove_user(ctx, env),
        (None, _) => prove_lead(ctx, env),
    }
}

/// The user's proof (ADR 0040). `is_member_session` is not enough here: it
/// sees no token when `TEAMWIRE_TOKEN_FILE` names a file it cannot read, and
/// that process is still a member's.
fn prove_user(ctx: &Ctx<'_>, env: &CallerEnv) -> Result<Actor, Refusal> {
    let set = |v: &Option<String>| v.as_deref().is_some_and(|v| !v.is_empty());
    if [&env.token, &env.token_file, &env.role, &env.team]
        .into_iter()
        .any(set)
    {
        return Err(Refusal::Identity);
    }
    let given = env.user_key.as_deref().ok_or(Refusal::Identity)?;
    let stored = std::fs::read_to_string(user_key_path(ctx.home)).map_err(|e| {
        tracing::warn!(error = %e, "the user key could not be read to prove the user");
        Refusal::Identity
    })?;
    let stored = stored.trim_ascii();
    if !stored.is_empty() && same_secret(stored, given.trim_ascii()) {
        Ok(Actor::User)
    } else {
        Err(Refusal::Identity)
    }
}

/// Compares two secrets in time that does not depend on where they differ.
/// Both are hashed first, so the compare also hides the given key's length.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn prove_member(ctx: &Ctx<'_>, env: &CallerEnv, token: &str) -> Result<Actor, Refusal> {
    let role = env
        .role
        .as_deref()
        .and_then(|r| Role::parse(r).ok())
        .ok_or(Refusal::Identity)?;
    if env
        .from
        .as_deref()
        .is_some_and(|from| from != role.as_str())
    {
        return Err(Refusal::Identity);
    }
    let found = ctx
        .store
        .read(|tx| Ok((tx.team()?.map(|t| t.name), tx.token_matches(&role, token)?)))
        .map_err(|e| {
            tracing::warn!(error = %e, "the store could not be read to prove a sender");
            Refusal::Identity
        })?;
    let (Some(team), Some(true)) = found else {
        return Err(Refusal::Identity);
    };
    if env.team.as_deref() == Some(team.as_str()) {
        Ok(Actor::Member(role))
    } else {
        Err(Refusal::Identity)
    }
}

fn prove_lead(ctx: &Ctx<'_>, env: &CallerEnv) -> Result<Actor, Refusal> {
    // A member's environment names a role. Without its token it proves nothing.
    if env.from.as_deref() != Some("lead") || env.is_member_session() {
        return Err(Refusal::Identity);
    }
    let team = read_team(ctx)
        .map_err(|e| {
            tracing::warn!(error = %e, "the store could not be read to prove the lead");
            Refusal::Identity
        })?
        .ok_or(Refusal::Identity)?;
    let (harness, _) = ctx.adapters(&Party::Lead).map_err(|_| Refusal::Identity)?;
    let session = harness
        .caller_session(&|name| env.lookup(name))
        .ok_or(Refusal::Identity)?;
    if session.kind == SessionKind::Interactive && team.lead_session.as_ref() == Some(&session.id) {
        Ok(Actor::Lead)
    } else {
        Err(Refusal::Identity)
    }
}
