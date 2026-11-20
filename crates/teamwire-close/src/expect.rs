//! The outcome checks of a whole-team close (ADR 0059).
//!
//! `--expect role:generation:status` names a result the lead needs before
//! the team may close. The close refuses unless that member is in that
//! generation and its newest result is in that generation with that
//! status. The check reads only the member's own recorded results: a verify
//! that passed or a check that was SKIPPED proves nothing about them.
//!
//! An ordinary close also refuses while a started member's current outcome
//! is one the lead must decide about: a `blocked` or `failed` result in its
//! current generation, or a current generation with no result after an
//! older one had one. A refused reprompt starts no generation, and a send
//! the client never made leaves no trace at all, so this reads the
//! outcomes, not the send history. An `--expect` naming that member
//! accepts its outcome; an expectation about another member does not.
//! `--abandon <why>` closes the team as it is, on purpose, with the reason
//! recorded. A close whose members all finished `done`, or never reported,
//! runs as before. The lead's refused newest send is still named too.
//!
//! Each started member's newest reprompt from the lead is checked on its
//! own. When it was refused, or its generation was given back, the member's
//! result answers an older task. Only a new reprompt to that member that
//! starts its generation answers it, or `--abandon`. A later message to
//! anyone else, a note to the member, or an `--expect` of the older result
//! does not.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use teamwire_core::{
    CompletionStatus, EventType, Member, MemberState, MessageType, Recipient, Role, TeamName,
};
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

/// The refusals of the outcome checks for a whole-team close with `opts`,
/// the same in the plan, the dry run and the re-check under the lock:
///
/// - each `--expect` that does not hold;
/// - without `--abandon`, each started member whose current outcome a
///   close must not archive unread: its result in its current generation
///   is `blocked` or `failed`, or its current generation has no result
///   while an older one has. Only an `--expect` naming that member
///   acknowledges it; an expectation about another member does not;
/// - without `--abandon`, a refused newest send by the lead, unless an
///   `--expect` names each member it was meant for;
/// - without `--abandon`, each started member whose newest reprompt from
///   the lead was refused or started no generation.
///
/// A store that cannot be read refuses, naming the error, since a check
/// that did not run proves nothing.
pub(crate) fn outcome_refusals(
    ctx: &Ctx<'_>,
    team: &teamwire_store::TeamRow,
    members: &[Member],
    opts: &crate::CloseOpts,
) -> Vec<String> {
    let found = (|| -> Result<Vec<String>, StoreError> {
        let mut out = unmet(ctx, members, &opts.expect)?;
        if opts.abandon.is_none() {
            out.extend(unacknowledged(ctx, &team.name, members, &opts.expect)?);
            out.extend(refused_send(ctx, &team.name, members, &opts.expect)?);
            out.extend(unanswered_reprompts(ctx, &team.name, members)?);
        }
        Ok(out)
    })();
    found.unwrap_or_else(|e| vec![format!("the outcome check could not read the store: {e}")])
}

/// The refusals of the expectations alone, for a team whose outcomes are
/// not checked any more (a closed team kept on disk).
pub(crate) fn expectation_refusals(
    ctx: &Ctx<'_>,
    members: &[Member],
    expected: &[Expected],
) -> Vec<String> {
    unmet(ctx, members, expected)
        .unwrap_or_else(|e| vec![format!("the outcome check could not read the store: {e}")])
}

/// The members a close would archive mid-story: started, with a current
/// result of `blocked` or `failed`, or with no result in a current
/// generation that follows one with a result. A member with no result at
/// all is not named: nothing of its story is in a result yet, and closing
/// such a team stays as it was.
fn unacknowledged(
    ctx: &Ctx<'_>,
    team: &TeamName,
    members: &[Member],
    expected: &[Expected],
) -> Result<Vec<String>, StoreError> {
    let mut out = Vec::new();
    for m in members.iter().filter(|m| m.state == MemberState::Started) {
        if expected.iter().any(|e| e.role == m.role) {
            continue;
        }
        let Some(newest) = ctx.store.read(|tx| tx.latest_completion(&m.role))? else {
            continue;
        };
        let c = &newest.completion;
        let role = &m.role;
        if c.generation == m.generation {
            if matches!(
                c.status,
                CompletionStatus::Blocked | CompletionStatus::Failed
            ) {
                out.push(format!(
                    "{role}'s current result is {status} (generation {g}): a close would archive \
                     it as the team's outcome. If that is what you mean, close with --expect \
                     {role}:{g}:{status}; to close the team as it is, use --abandon \"<why>\"; \
                     otherwise check `teamwire status {team}` and finish the work first",
                    status = c.status,
                    g = c.generation,
                ));
            }
        } else {
            out.push(format!(
                "{role} is in generation {now} with no result yet (its newest is generation \
                 {old}, {status}): a close would archive the older result as if it answered the \
                 newer work. Wait for its result and close with --expect {role}:{now}:<status>, \
                 or use --abandon \"<why>\"",
                now = m.generation,
                old = c.generation,
                status = c.status,
            ));
        }
    }
    Ok(out)
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

/// The refusal of a close whose lead's newest send was refused, with
/// nothing the lead stored after it. `None` when the lead's newest send was
/// stored, when the lead never had one refused, or when an expectation
/// names each member the send was meant for. A send that never reached
/// teamwire leaves no row, so this check alone is not enough: the current
/// outcomes are checked whatever the send history holds.
pub(crate) fn refused_send(
    ctx: &Ctx<'_>,
    team: &TeamName,
    members: &[Member],
    expected: &[Expected],
) -> Result<Option<String>, StoreError> {
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
    // The members the send was meant for: those it named, or every started
    // member when it named none of them (`members`, `lead`, nothing). Each
    // needs an expectation before the refusal is answered.
    let started: Vec<&Member> = members
        .iter()
        .filter(|m| m.state == MemberState::Started)
        .collect();
    let named: Vec<&Member> = started
        .iter()
        .copied()
        .filter(|m| to.contains(&m.role.as_str()))
        .collect();
    let meant = if named.is_empty() { started } else { named };
    if !meant.is_empty()
        && meant
            .iter()
            .all(|m| expected.iter().any(|e| e.role == m.role))
    {
        return Ok(None);
    }
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

/// The lead's newest reprompt to one member, as the store holds it.
enum Newest {
    /// The rules refused it: it was never stored, and started nothing.
    Refused { code: String, at: String },
    /// It was stored as message `id`.
    Stored { id: String },
}

/// The refusals for each started member whose newest reprompt from the lead
/// is unanswered: it was refused, or it was stored but started no
/// generation, because its delivery failed and the generation was given
/// back (ADR 0059). Each member is checked on its own. A note, a finding or
/// a reprompt to another member is no answer, and an `--expect` of the
/// member's older result is none either: only a new reprompt to the same
/// member that starts its generation is, and the outcome checks then wait
/// for that generation's result. A refused send whose type was never read
/// counts as a reprompt, since it may have been one. A refused send that
/// named no started member was meant for each of them.
pub(crate) fn unanswered_reprompts(
    ctx: &Ctx<'_>,
    team: &TeamName,
    members: &[Member],
) -> Result<Vec<String>, StoreError> {
    let started: Vec<&Member> = members
        .iter()
        .filter(|m| m.state == MemberState::Started)
        .collect();
    ctx.store.read(|tx| {
        let mut newest: Vec<Option<(u64, Newest)>> = started.iter().map(|_| None).collect();
        let mut note = |at: usize, seq: u64, what: Newest| {
            if let Some(slot) = newest.get_mut(at)
                && slot.as_ref().is_none_or(|(s, _)| *s < seq)
            {
                *slot = Some((seq, what));
            }
        };
        let text = |e: &teamwire_core::Event, key: &str| {
            e.data.get(key).and_then(Value::as_str).map(str::to_owned)
        };
        for e in tx.events_of_type(EventType::MessageRefused)?.items {
            let kind = text(&e, "type");
            if text(&e, "sender").as_deref() != Some("lead")
                || kind.is_some_and(|k| k != MessageType::Reprompt.as_str())
            {
                continue;
            }
            let to: Vec<&str> = e
                .data
                .get("to")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let named = started.iter().any(|m| to.contains(&m.role.as_str()));
            for (at, m) in started.iter().enumerate() {
                if !named || to.contains(&m.role.as_str()) {
                    let code = text(&e, "code").unwrap_or_else(|| "unknown".to_owned());
                    note(
                        at,
                        e.seq,
                        Newest::Refused {
                            code,
                            at: e.time.to_string(),
                        },
                    );
                }
            }
        }
        for e in tx.events_of_type(EventType::MessageSent)?.items {
            if text(&e, "sender_kind").as_deref() != Some("lead") {
                continue;
            }
            let Some(ulid) = e.subject else { continue };
            let Some(message) = tx.message(&ulid)? else {
                continue;
            };
            if message.kind != MessageType::Reprompt {
                continue;
            }
            for (at, m) in started.iter().enumerate() {
                if tx
                    .delivery(&ulid, &Recipient::Member(m.role.clone()))?
                    .is_some()
                {
                    let id = message.display_id.as_str().to_owned();
                    note(at, e.seq, Newest::Stored { id });
                }
            }
        }
        let mut out = Vec::new();
        for (m, newest) in started.iter().zip(newest) {
            let role = &m.role;
            let what = match newest {
                None => continue,
                Some((_, Newest::Stored { id })) => {
                    if tx.generation_started_by(role, &id)?.is_some() {
                        continue;
                    }
                    format!(
                        "{id}, started no generation: its delivery failed and the generation \
                         was given back"
                    )
                }
                Some((_, Newest::Refused { code, at })) => {
                    format!("was refused ({code}, at {at}) and started no generation")
                }
            };
            out.push(format!(
                "{role}'s newest reprompt from the lead{sep}{what}, so {role}'s result in \
                 generation {g} answers an older task. A note, a reprompt to another member or \
                 an --expect of that older result does not answer it: reprompt {role} again and \
                 wait for its result (check with `teamwire status {team}`), or close with \
                 --abandon \"<why>\" to close the team as it is",
                sep = if what.starts_with('m') { ", " } else { " " },
                g = m.generation,
            ));
        }
        Ok(out)
    })
}
