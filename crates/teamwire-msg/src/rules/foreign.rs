//! The rules for a recipient in another team (MSG-20, ADR 0016). That team's
//! store is opened read-only, so a refused send writes nothing anywhere.
//! Another team informs and never instructs: a reprompt or a note goes only
//! to the sender's own team, and a reviewer or closer hears only its own lead.

use std::collections::BTreeMap;

use teamwire_core::{
    Actor, MemberKind, MemberState, MessageType, Recipient, Refusal, Remote, SendRequest, TeamName,
    TeamState,
};
use teamwire_store::{Ctx, Store, StoreError, TeamRow};

use crate::SendError;

/// Checks every recipient of `req` that sits in another team. `own` is the
/// sender's team row.
pub(super) fn check(
    ctx: &Ctx<'_>,
    own: &TeamRow,
    actor: &Actor,
    req: &SendRequest,
) -> Result<(), SendError> {
    let mut by_team: BTreeMap<&str, Vec<&Remote>> = BTreeMap::new();
    for to in &req.to {
        if let Recipient::Other(remote) = to {
            if matches!(req.kind, MessageType::Reprompt | MessageType::Note) {
                return Err(Refusal::RecipientNotAllowed.into());
            }
            by_team
                .entry(remote.team().as_str())
                .or_default()
                .push(remote);
        }
    }
    for remotes in by_team.values() {
        let Some(team) = remotes.first().map(|r| r.team()) else {
            continue;
        };
        let (row, members) = read_team(ctx, team)?;
        if row.state != TeamState::Open {
            return Err(Refusal::RecipientTeamClosed.into());
        }
        for remote in remotes {
            match remote.role() {
                // One lead session can lead both teams; its own pane is no
                // one else's.
                None => {
                    if *actor == Actor::Lead
                        && row.lead_session.is_some()
                        && row.lead_session == own.lead_session
                    {
                        return Err(Refusal::SelfMessage.into());
                    }
                }
                Some(role) => {
                    let member = members
                        .iter()
                        .find(|m| &m.role == role)
                        .ok_or(Refusal::UnknownRecipient)?;
                    if member.state == MemberState::Removed {
                        return Err(Refusal::RemovedRecipient.into());
                    }
                    if matches!(member.kind, MemberKind::Reviewer | MemberKind::Closer) {
                        return Err(Refusal::RecipientNotAllowed.into());
                    }
                }
            }
        }
    }
    Ok(())
}

/// That team's row and members. No database is no such team.
fn read_team(
    ctx: &Ctx<'_>,
    team: &TeamName,
) -> Result<(TeamRow, Vec<teamwire_core::Member>), SendError> {
    let store = match Store::open_read_only(ctx.home, team) {
        Ok(store) => store,
        Err(StoreError::NoDatabase { .. }) => return Err(Refusal::UnknownTeam.into()),
        Err(e) => return Err(e.into()),
    };
    let (row, members) = store.read(|tx| Ok((tx.team()?, tx.members()?.all_or_corrupt()?)))?;
    let row = row.ok_or(Refusal::UnknownTeam)?;
    Ok((row, members))
}
