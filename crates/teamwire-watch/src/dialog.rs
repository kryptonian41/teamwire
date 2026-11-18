//! The watch lines for a member at a dialog (ADR 0052): one line per
//! permission dialog the member's hook recorded, and one per blocked episode
//! for any other dialog, named by its kind.

use teamwire_core::{DialogKind, Role, clock_time, shown};
use teamwire_store::DialogRecord;

use crate::text::agent_text;

/// The longest summary a dialog line shows, in characters.
const SUMMARY_SHOWN: usize = 160;

/// The announcement key of one recorded dialog: one line per dialog.
pub(crate) fn dialog_key(dialog: &DialogRecord) -> String {
    format!("DIALOG:{}", dialog.id)
}

/// `BLOCKED api permission Bash: rmdir /usr (dialog <id>, answer by 09:50:44Z)`.
/// The summary is the member's model's text, so it goes through `shown`.
pub(crate) fn dialog_line(role: &Role, dialog: &DialogRecord) -> String {
    format!(
        "BLOCKED {role} permission {}: {} (dialog {}, answer by {})",
        agent_text(&shown(&dialog.request.tool), 40),
        agent_text(&shown(&dialog.request.summary), SUMMARY_SHOWN),
        dialog.id,
        clock_time(dialog.expires_at)
    )
}

/// `BLOCKED <role> <kind>` for a dialog no pending record covers.
pub(crate) fn blocked_line(role: &Role, kind: DialogKind) -> String {
    let kind = match kind {
        DialogKind::Permission => "permission (teamwire cannot answer it)",
        DialogKind::Trust => "trust",
        DialogKind::Login => "login",
        DialogKind::Update => "update",
        DialogKind::Unknown | DialogKind::NoDialog | DialogKind::CannotTell => "unknown",
    };
    format!("BLOCKED {role} {kind}")
}
