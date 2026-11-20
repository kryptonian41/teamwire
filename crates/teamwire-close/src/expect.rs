//! The outcome checks of a whole-team close (ADR 0057).
//!
//! `--expect role:generation:status` names a result the lead needs before
//! the team may close. The close refuses unless that member is in that
//! generation and its newest result is in that generation with that
//! status. The check reads only the member's own recorded results: a verify
//! that passed or a check that was SKIPPED proves nothing about them.
//!
//! An ordinary close also refuses, in one case: the lead's newest send was
//! refused and it stored no message after it. A refused reprompt starts no
//! generation, so the team that close would archive is not the one the
//! lead tried to make. `--expect` checks the outcome instead, and
//! `--abandon <why>` closes the team as it is, on purpose, with the reason
//! recorded. Every other ordinary close runs as before.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use teamwire_core::{CompletionStatus, EventType, Member, MemberState, Role, TeamName};
use teamwire_store::{Ctx, StoreError};

/// One `--expect role:generation:status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    pub role: Role,
    pub generation: u32,
    pub status: CompletionStatus,
}

impl Expected {
    /// Reads `role:generation:status`. The status is one a member records
    /// (`done`, `blocked`, `failed`): a word such as `skipped` or `clean`
    /// is no result and is refused.
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<&str> = text.split(':').collect();
        let [role, generation, status] = parts.as_slice() else {
            return Err(format!(
                "--expect {text}: write role:generation:status, such as api:4:done"
            ));
        };
        let role = Role::parse(role).map_err(|e| format!("--expect {text}: {e}"))?;
        let generation = generation
            .parse::<u32>()
            .ok()
            .filter(|g| *g > 0)
            .ok_or_else(|| format!("--expect {text}: the generation must be a number from 1"))?;
        let status = CompletionStatus::parse(status).map_err(|_| {
            format!("--expect {text}: the status must be done, blocked or failed, a result a member records")
        })?;
        Ok(Self {
            role,
            generation,
            status,
        })
    }
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.role, self.generation, self.status)
    }
}

/// The refusals of the outcome checks for a whole-team close with `opts`:
/// the unmet expectations when `--expect` is given; with neither `--expect`
/// nor `--abandon`, the refused newest send. A store that cannot be read
/// refuses, naming the error, since a check that did not run proves nothing.
pub(crate) fn outcome_refusals(
    ctx: &Ctx<'_>,
    team: &teamwire_store::TeamRow,
    members: &[Member],
    opts: &crate::CloseOpts,
) -> Vec<String> {
    let found = if !opts.expect.is_empty() {
        unmet(ctx, members, &opts.expect)
    } else if opts.abandon.is_none() {
        refused_send(ctx, &team.name).map(|r| r.into_iter().collect())
    } else {
        Ok(Vec::new())
    };
    found.unwrap_or_else(|e| vec![format!("the outcome check could not read the store: {e}")])
}

/// Why each expectation does not hold now; empty when all hold.
pub(crate) fn unmet(
    ctx: &Ctx<'_>,
    members: &[Member],
    expected: &[Expected],
) -> Result<Vec<String>, StoreError> {
    let mut out = Vec::new();
    for e in expected {
        let Some(member) = members.iter().find(|m| m.role == e.role) else {
            out.push(format!("--expect {e}: the team has no member {}", e.role));
            continue;
        };
        if member.state == MemberState::Removed {
            out.push(format!("--expect {e}: {} was removed", e.role));
            continue;
        }
        if member.generation != e.generation {
            out.push(format!(
                "--expect {e}: {} is in generation {}, not {}; a reprompt that was refused or \
                 never sent starts no generation",
                e.role, member.generation, e.generation
            ));
            continue;
        }
        let newest = ctx.store.read(|tx| tx.latest_completion(&e.role))?;
        match newest {
            Some(r) if r.completion.generation == e.generation => {
                if r.completion.status != e.status {
                    out.push(format!(
                        "--expect {e}: {}'s result in generation {} is {}, not {}",
                        e.role, e.generation, r.completion.status, e.status
                    ));
                }
            }
            Some(r) => out.push(format!(
                "--expect {e}: {} has no result in generation {} (its newest is generation {}, {})",
                e.role, e.generation, r.completion.generation, r.completion.status
            )),
            None => out.push(format!("--expect {e}: {} has recorded no result", e.role)),
        }
    }
    Ok(out)
}

/// The refusal of an ordinary close whose lead's newest send was refused,
/// with nothing the lead stored after it. `None` when the lead's newest
/// send was stored, or the lead never had one refused.
pub(crate) fn refused_send(ctx: &Ctx<'_>, team: &TeamName) -> Result<Option<String>, StoreError> {
    let by_lead = |e: &teamwire_core::Event, key: &str| {
        e.data.get(key).and_then(Value::as_str) == Some("lead")
    };
    let (refused, sent) = ctx.store.read(|tx| {
        let refused = tx
            .events_of_type(EventType::MessageRefused)?
            .items
            .into_iter()
            .rfind(|e| by_lead(e, "sender"));
        let sent = tx
            .events_of_type(EventType::MessageSent)?
            .items
            .into_iter()
            .rfind(|e| by_lead(e, "sender_kind"))
            .map(|e| e.seq);
        Ok((refused, sent))
    })?;
    let Some(refused) = refused.filter(|r| sent.is_none_or(|s| s < r.seq)) else {
        return Ok(None);
    };
    let field = |key: &str| {
        refused
            .data
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned()
    };
    let to: Vec<&str> = refused
        .data
        .get("to")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let to = if to.is_empty() {
        "no recipient".to_owned()
    } else {
        to.join(", ")
    };
    Ok(Some(format!(
        "the lead's newest send was refused ({code}; type {kind}; to {to}; at {at}) and the lead \
         stored no message after it: a refused send starts no generation, so the results this \
         close would archive may not be the ones the lead asked for. Check each result with \
         `teamwire status {team}`, then close with --expect <role>:<generation>:<status> for each \
         result you need, or with --abandon \"<why>\" to close the team as it is",
        code = field("code"),
        kind = field("type"),
        at = refused.time,
    )))
}
