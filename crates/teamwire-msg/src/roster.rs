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
    /// The stored lifecycle (`creating`, `started`, `failed`, `removed`).
    /// It does not say whether the member is working or finished: that is
    /// the live `state` of `team_status`, so this field has its own name.
    pub lifecycle: MemberState,
    pub generation: u32,
    pub owns: Vec<String>,
    /// The message types this member accepts from another member, of this
    /// team or another. Every member also accepts its own lead's `note` and
    /// `reprompt`, which this list leaves out. Its name says so, because a
    /// caller sees only the name.
    pub accepts_from_members: Vec<MessageType>,
    /// Whether typing into the member's session can wake it.
    pub can_be_typed_to: bool,
}

/// Every member, in the order they joined. A reviewer or closer accepts
/// messages only from its own lead (MSG-3); every other member accepts every
/// kind of information from anyone (MSG-1).
pub fn roster(ctx: &Ctx<'_>) -> Result<Vec<RosterEntry>, SendError> {
    let members = ctx.store.read(|tx| tx.members()?.all_or_corrupt())?;
    Ok(members
        .into_iter()
        .map(|m| {
            let accepts_from_members = match m.kind {
                MemberKind::Reviewer | MemberKind::Closer => Vec::new(),
                MemberKind::Implementer | MemberKind::Investigator => vec![
                    MessageType::Notice,
                    MessageType::Finding,
                    MessageType::Question,
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
                lifecycle: m.state,
                generation: m.generation,
                owns: m.owns,
                accepts_from_members,
                can_be_typed_to,
            }
        })
        .collect())
}
