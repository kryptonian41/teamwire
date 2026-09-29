//! `members`: who is on the team, as `team_members` returns it. No place,
//! token or session id.

use serde::{Deserialize, Serialize};
use teamwire_core::{MemberKind, MemberState, MessageType, Role};
use teamwire_store::Ctx;

use crate::SendError;

/// One member of the roster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub role: Role,
    pub title: String,
    pub kind: MemberKind,
    pub state: MemberState,
    pub generation: u32,
    pub owns: Vec<String>,
    /// The message types this member accepts from another member.
    pub accepts: Vec<MessageType>,
    /// Whether typing into the member's session can wake it.
    pub can_be_typed_to: bool,
}

/// Every member, in the order they joined. A reviewer or closer accepts
/// messages only from the lead (MSG-3); a question is accepted only by an
/// investigator.
pub fn roster(ctx: &Ctx<'_>) -> Result<Vec<RosterEntry>, SendError> {
    let members = ctx.store.read(|tx| tx.members()?.all_or_corrupt())?;
    Ok(members
        .into_iter()
        .map(|m| {
            let accepts = match m.kind {
                MemberKind::Reviewer | MemberKind::Closer => Vec::new(),
                MemberKind::Investigator => vec![
                    MessageType::Notice,
                    MessageType::Finding,
                    MessageType::Question,
                    MessageType::Answer,
                ],
                MemberKind::Implementer => vec![
                    MessageType::Notice,
                    MessageType::Finding,
                    MessageType::Answer,
                ],
            };
            let can_be_typed_to = ctx
                .registry
                .host(&m.host)
                .is_ok_and(|host| host.capabilities().can_type);
            RosterEntry {
                role: m.role,
                title: m.title,
                kind: m.kind,
                state: m.state,
                generation: m.generation,
                owns: m.owns,
                accepts,
                can_be_typed_to,
            }
        })
        .collect())
}
