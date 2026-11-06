//! The rules a send must pass before anything is stored (MSG-1..MSG-3,
//! MSG-9, MSG-14, MSG-15, MSG-20): who may send which type to whom, size
//! limits, the notice's commit, an answer's question, the burst and repeat
//! limits. A recipient in another team is checked against that team's
//! store in `foreign`.

mod foreign;

use std::path::Path;
use std::time::Duration;

use teamwire_core::{
    Actor, Command, DeliveryState, DisplayId, Member, MemberKind, MemberState, Message,
    MessageType, Recipient, Refusal, SendRequest, TeamState, Timestamp, Ulid,
};
use teamwire_store::{Ctx, TeamRow};

use crate::SendError;

/// At most this many messages from one sender to one recipient per minute.
const BURST_LIMIT: usize = 5;
const BURST_WINDOW_MS: i64 = 60_000;
/// An identical body to the same recipient inside this window is a repeat.
const REPEAT_WINDOW_MS: i64 = 600_000;
const GIT_TIMEOUT: Duration = Duration::from_secs(30);

/// What the rules resolved while checking, so the send does not look it up again.
pub(crate) struct Checked {
    pub(crate) recipients: Vec<Recipient>,
    /// The question an answer answers.
    pub(crate) re: Option<Ulid>,
    /// The commit as git names it, for a notice.
    pub(crate) commit: Option<String>,
}

/// The stored facts the pure rules need, read in one snapshot.
struct Snapshot {
    team: TeamRow,
    team_state: TeamState,
    members: Vec<Member>,
    /// The question named by `re`, and who it was asked of.
    question: Option<(Message, Vec<Recipient>)>,
}

/// The largest body for a type; `None` means no limit (MSG-1).
pub(crate) fn limit_of(kind: MessageType) -> Option<usize> {
    match kind {
        MessageType::Notice => Some(600),
        MessageType::Finding | MessageType::Question | MessageType::Answer => Some(800),
        MessageType::Reprompt | MessageType::Note => None,
    }
}

/// Checks `req` against the team's rules. Nothing is written.
pub(crate) fn check(ctx: &Ctx<'_>, req: &SendRequest) -> Result<Checked, SendError> {
    let re_id = match (req.kind, &req.re) {
        (MessageType::Answer, None) => {
            return Err(SendError::InvalidArgument(
                "an answer needs re, the question's id".to_owned(),
            ));
        }
        (MessageType::Answer, Some(id)) => Some(id),
        (
            MessageType::Notice
            | MessageType::Finding
            | MessageType::Question
            | MessageType::Reprompt
            | MessageType::Note,
            _,
        ) => None,
    };
    let snap = snapshot(ctx, re_id)?;
    let sender_kind = check_sender(&snap, &req.actor)?;
    check_type(sender_kind, req.kind)?;
    check_size(req)?;
    check_recipients(&snap, &req.actor, req)?;
    foreign::check(ctx, &snap.team, &req.actor, req)?;
    let re = check_answer(&snap, req)?;
    let commit = if req.kind == MessageType::Notice {
        Some(check_commit(ctx, &snap, req)?)
    } else {
        None
    };
    if !is_resume(ctx, req)? {
        check_rate_and_repeat(ctx, req)?;
    }
    Ok(Checked {
        recipients: req.to.clone(),
        re,
        commit,
    })
}

fn snapshot(ctx: &Ctx<'_>, re: Option<&DisplayId>) -> Result<Snapshot, SendError> {
    Ok(ctx.store.read(|tx| {
        let team = tx.team()?.ok_or_else(|| {
            teamwire_store::StoreError::Corrupt("the store has no team row".into())
        })?;
        let question = match re {
            Some(id) => match tx.message_by_display_id(id)? {
                Some(m) => {
                    let to = tx
                        .deliveries_of(&m.ulid)?
                        .all_or_corrupt()?
                        .into_iter()
                        .map(|d| d.delivery.recipient)
                        .collect();
                    Some((m, to))
                }
                None => None,
            },
            None => None,
        };
        Ok(Snapshot {
            team_state: team.state,
            team,
            members: tx.members()?.all_or_corrupt()?,
            question,
        })
    })?)
}

/// Who is speaking: `None` for the lead.
type SenderKind = Option<MemberKind>;

fn check_sender(snap: &Snapshot, actor: &Actor) -> Result<SenderKind, SendError> {
    if snap.team_state != TeamState::Open {
        return Err(Refusal::TeamClosed.into());
    }
    match actor {
        Actor::Lead => Ok(None),
        // Nothing proves the user yet.
        Actor::User => Err(Refusal::Identity.into()),
        // A caller is proven in its own team, never as another team's agent.
        Actor::Other(_) => Err(Refusal::Identity.into()),
        Actor::Member(role) => {
            // The actor is proven, so a role with no live row is a stale proof.
            let member = snap
                .members
                .iter()
                .find(|m| &m.role == role && m.state != MemberState::Removed)
                .ok_or(Refusal::Identity)?;
            Ok(Some(member.kind))
        }
    }
}

/// Every agent that talks sends every kind of information; only the lead
/// instructs (MSG-1).
fn check_type(sender: SenderKind, kind: MessageType) -> Result<(), SendError> {
    let allowed = match (sender, kind) {
        (Some(k), _) => k.may_send(kind),
        // A notice names a commit in the sender's checkout, and the lead has
        // no checkout on record to look it up in (MSG-2).
        (None, MessageType::Notice) => false,
        (
            None,
            MessageType::Finding
            | MessageType::Question
            | MessageType::Answer
            | MessageType::Reprompt
            | MessageType::Note,
        ) => true,
    };
    if allowed {
        Ok(())
    } else {
        Err(Refusal::TypeNotAllowed.into())
    }
}

fn check_size(req: &SendRequest) -> Result<(), SendError> {
    if req.body.trim().is_empty() {
        return Err(Refusal::EmptyBody.into());
    }
    let got = req.body.chars().count();
    match limit_of(req.kind) {
        Some(limit) if got > limit => Err(Refusal::TooLong { limit, got }.into()),
        _ => Ok(()),
    }
}

fn check_recipients(snap: &Snapshot, actor: &Actor, req: &SendRequest) -> Result<(), SendError> {
    if req.to.is_empty() {
        return Err(SendError::InvalidArgument(
            "a message needs at least one recipient".to_owned(),
        ));
    }
    let from_lead = *actor == Actor::Lead;
    for to in &req.to {
        if to.as_str() == actor.as_str() {
            return Err(Refusal::SelfMessage.into());
        }
        let member = match to {
            Recipient::Lead => None,
            // Checked against its own team's store in `foreign`.
            Recipient::Other(_) => continue,
            Recipient::Member(role) => {
                let found = snap
                    .members
                    .iter()
                    .find(|m| &m.role == role)
                    .ok_or(Refusal::UnknownRecipient)?;
                if found.state == MemberState::Removed {
                    return Err(Refusal::RemovedRecipient.into());
                }
                Some(found)
            }
        };
        if let Some(m) = member {
            // A fresh view of the work: only the lead speaks to these (MSG-3).
            if m.kind.is_quiet() && !from_lead {
                return Err(Refusal::RecipientNotAllowed.into());
            }
            // A generation can only start for a member that is running.
            if req.kind == MessageType::Reprompt
                && !matches!(m.state, MemberState::Creating | MemberState::Started)
            {
                return Err(Refusal::RecipientNotAllowed.into());
            }
        }
    }
    Ok(())
}

/// An answer answers a real question that was asked of the answerer and
/// goes only to the asker (MSG-9).
fn check_answer(snap: &Snapshot, req: &SendRequest) -> Result<Option<Ulid>, SendError> {
    if req.kind != MessageType::Answer {
        return Ok(None);
    }
    let Some((question, asked)) = &snap.question else {
        return Err(Refusal::NotAQuestion.into());
    };
    if question.kind != MessageType::Question {
        return Err(Refusal::NotAQuestion.into());
    }
    let me = Recipient::parse(req.actor.as_str()).map_err(|_| Refusal::Identity)?;
    if !asked.contains(&me) {
        return Err(Refusal::NotAskedOfYou.into());
    }
    let asker = Recipient::parse(question.sender.as_str()).map_err(|_| Refusal::Identity)?;
    if req.to != [asker] {
        return Err(Refusal::RecipientNotAllowed.into());
    }
    Ok(Some(question.ulid))
}

/// A notice names a commit that exists in the sender's checkout and is an
/// ancestor of its current head (MSG-2). Returns the full hash git gives it.
fn check_commit(ctx: &Ctx<'_>, snap: &Snapshot, req: &SendRequest) -> Result<String, SendError> {
    let commit = req.commit.as_deref().ok_or(Refusal::NoCommit)?.trim();
    if commit.is_empty() {
        return Err(Refusal::NoCommit.into());
    }
    // A leading dash would be read by git as an option.
    if commit.starts_with('-') || commit.len() > 64 || commit.chars().any(char::is_whitespace) {
        return Err(Refusal::CommitNotReachable.into());
    }
    let Actor::Member(role) = &req.actor else {
        return Err(Refusal::TypeNotAllowed.into());
    };
    let cwd = snap
        .members
        .iter()
        .find(|m| &m.role == role)
        .map(|m| m.cwd.as_path())
        .ok_or(Refusal::Identity)?;
    let found = git(
        ctx,
        cwd,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{commit}^{{commit}}"),
        ],
    )?;
    let full = String::from_utf8_lossy(&found.stdout).trim().to_owned();
    if !found.success() || full.is_empty() {
        return Err(Refusal::CommitNotReachable.into());
    }
    if !git(ctx, cwd, &["merge-base", "--is-ancestor", &full, "HEAD"])?.success() {
        return Err(Refusal::CommitNotReachable.into());
    }
    Ok(full)
}

fn git(ctx: &Ctx<'_>, cwd: &Path, args: &[&str]) -> Result<teamwire_core::RunOutput, SendError> {
    let cmd = Command::new("git").args(args.iter().copied()).cwd(cwd);
    ctx.platform
        .run(&cmd, GIT_TIMEOUT)
        .map_err(|e| SendError::Internal(format!("git could not run: {e}")))
}

/// A send whose key already names a message resumes it. The burst and repeat
/// limits are for new messages, not for finishing one.
fn is_resume(ctx: &Ctx<'_>, req: &SendRequest) -> Result<bool, SendError> {
    match &req.client_key {
        Some(key) => Ok(ctx
            .store
            .read(|tx| tx.message_by_client_key(&req.actor, key))?
            .is_some()),
        None => Ok(false),
    }
}

fn earlier(now: Timestamp, ms: i64) -> Timestamp {
    Timestamp::from_unix_millis(now.unix_millis().saturating_sub(ms)).unwrap_or(now)
}

/// At most 5 messages per sender to one recipient per minute, and an
/// identical body to the same recipient within 10 minutes is refused unless
/// the earlier copy was never delivered (MSG-14, ADR 0003 point 12).
fn check_rate_and_repeat(ctx: &Ctx<'_>, req: &SendRequest) -> Result<(), SendError> {
    let now = ctx.clock.now();
    for to in &req.to {
        let found = ctx
            .store
            .read(|tx| tx.sent_to_since(&req.actor, to, earlier(now, REPEAT_WINDOW_MS)))?;
        // A damaged old message must not stop new ones: these two limits are
        // advisory, and `verify --deep` names the damaged row.
        for bad in &found.bad {
            tracing::warn!(key = %bad.key, reason = %bad.reason, "skipped a damaged message in the burst check");
        }
        let recent = found.items;
        let in_minute = recent
            .iter()
            .filter(|r| r.message.created_at >= earlier(now, BURST_WINDOW_MS))
            .count();
        if in_minute >= BURST_LIMIT {
            return Err(Refusal::RateLimited.into());
        }
        let repeated = recent.iter().any(|r| {
            r.message.body == req.body
                && !matches!(r.state, DeliveryState::Undelivered | DeliveryState::Pending)
        });
        if repeated {
            return Err(Refusal::Repeat.into());
        }
    }
    Ok(())
}

impl From<Refusal> for SendError {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}
