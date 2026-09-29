//! `log`: every message with each recipient's delivery state.

use teamwire_core::{Actor, Message, Recipient, one_line};
use teamwire_store::{Ctx, MessageFilter};

use crate::attempt::report_of;
use crate::{LogFilter, LogRow, RecipientReport, SendError};

/// The longest excerpt a row carries.
const EXCERPT_CHARS: usize = 300;

/// The messages the viewer may see, oldest first. A member sees only
/// messages it sent or received; the lead sees all. `role` keeps only
/// messages to or from that role. `limit` keeps the newest rows.
///
/// A message whose body fails its hash cannot be shown as it is stored. It
/// is left out here and `verify --deep` names it.
pub fn log(ctx: &Ctx<'_>, filter: LogFilter) -> Result<Vec<LogRow>, SendError> {
    let LogFilter {
        viewer,
        role,
        limit,
    } = filter;
    let viewer_scope = match viewer {
        Actor::Lead => None,
        Actor::Member(role) => Some(Actor::Member(role)),
    };
    let query = MessageFilter {
        involving: viewer_scope
            .clone()
            .or_else(|| role.clone().map(Actor::Member)),
        ..MessageFilter::default()
    };
    let mut rows = ctx.store.read(|tx| {
        let found = tx.messages(&query)?;
        for bad in &found.bad {
            tracing::warn!(key = %bad.key, reason = %bad.reason, "a message is left out of the log");
        }
        let mut rows = Vec::new();
        for message in found.items {
            let deliveries = tx.deliveries_of(&message.ulid)?.all_or_corrupt()?;
            rows.push((message, deliveries));
        }
        Ok(rows)
    })?;
    if let (Some(role), Some(_)) = (&role, &viewer_scope) {
        // The store filters one party; a member's own view is narrowed to the role here.
        rows.retain(|(m, d)| involves(m, d.iter().map(|r| &r.delivery.recipient), role));
    }
    if let Some(limit) = limit {
        let extra = rows.len().saturating_sub(limit);
        rows.drain(..extra);
    }
    Ok(rows
        .into_iter()
        .map(|(m, records)| {
            let deliveries: Vec<RecipientReport> = records.iter().map(report_of).collect();
            LogRow {
                display_id: m.display_id,
                time: m.created_at,
                kind: m.kind,
                from: m.sender,
                to: deliveries.iter().map(|d| d.to.clone()).collect(),
                excerpt: one_line(&m.body, EXCERPT_CHARS).into_string(),
                deliveries,
            }
        })
        .collect())
}

fn involves<'a>(
    message: &Message,
    mut to: impl Iterator<Item = &'a Recipient>,
    role: &teamwire_core::Role,
) -> bool {
    message.sender.as_str() == role.as_str() || to.any(|r| r.as_str() == role.as_str())
}
