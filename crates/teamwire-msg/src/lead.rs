//! `lead --adopt`: a new lead session takes over a team (ADR 0008 point 3).

use teamwire_core::{SessionId, SessionKind};
use teamwire_store::{Ctx, Party};

use crate::{CallerEnv, SendError};
use teamwire_core::Refusal;

/// What adoption changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptReport {
    pub old: Option<SessionId>,
    pub new: SessionId,
}

/// Records the calling session as the team's lead and writes
/// `team.lead_adopted`. Refused with `identity` when the caller holds a
/// member's token, or its harness cannot name an interactive session: a
/// process that cannot say who it is must not become the lead.
pub fn adopt_lead(ctx: &mut Ctx<'_>, env: &CallerEnv) -> Result<AdoptReport, SendError> {
    if env.token.is_some() {
        return Err(Refusal::Identity.into());
    }
    let session = {
        let (harness, _) = ctx.adapters(&Party::Lead).map_err(|e| {
            SendError::Internal(format!("the lead's harness is not available: {e}"))
        })?;
        harness.current_session().ok_or(Refusal::Identity)?
    };
    if session.kind != SessionKind::Interactive {
        return Err(Refusal::Identity.into());
    }
    let now = ctx.clock.now();
    let old = ctx
        .store
        .write(|tx| tx.adopt_lead(now, &session.id, session.kind))?;
    Ok(AdoptReport {
        old,
        new: session.id,
    })
}
