//! When a `handed_off` delivery counts as unread for the lead's views: the
//! watch's `UNREAD` line and `teamwire status`. The member's own hooks keep
//! their rule (ADR 0021), because a hold before stopping is how a member is
//! asked to read the full text at all.

use teamwire_core::{Message, Recipient, Timestamp};
use teamwire_store::{ReadTx, StoreError};

use crate::{attempt_started, line_holds_whole};

/// When the latest attempt at a `handed_off` delivery began, or `None` when
/// it is not unread: the typed line held the whole body (ADR 0021), or the
/// member recorded a completion after the line reached it (ADR 0022).
pub fn unread_since(
    tx: &ReadTx<'_>,
    message: &Message,
    recipient: &Recipient,
) -> Result<Option<Timestamp>, StoreError> {
    if line_holds_whole(&message.body) {
        return Ok(None);
    }
    let since = attempt_started(tx, message, recipient)?;
    if let Recipient::Member(role) = recipient {
        // The member finished a turn that had the line: no later nudge
        // exists, so an alert would only send the lead to check by hand.
        if tx
            .latest_completion(role)?
            .is_some_and(|c| c.completion.created_at > since)
        {
            return Ok(None);
        }
    }
    Ok(Some(since))
}
