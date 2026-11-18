//! The watch lines for a member at a dialog (ADR 0052): one line per
//! permission dialog the member's hook recorded, and one per blocked episode
//! for any other dialog, named by its kind.

use std::time::Duration;

use teamwire_core::{
    DialogKind, DialogState, PERMISSION_COUNTDOWN, Role, Timestamp, clock_time, shown,
};
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

/// True while a dialog whose hook gave up can still be denied by the
/// harness's own countdown. The hook stops waiting before the countdown
/// ends, and the dialog stays on the screen for the time in between. The
/// member then moves again without anyone, so it is not stuck yet. One poll
/// more than the countdown covers a host that reports the closed dialog a
/// moment late. A dialog still on the screen after that has no countdown,
/// and only a person can answer it.
pub(crate) fn countdown_runs(dialogs: &[DialogRecord], now: Timestamp, poll: Duration) -> bool {
    let settled_after = PERMISSION_COUNTDOWN.saturating_add(poll);
    dialogs.iter().any(|d| {
        d.state == DialogState::Expired && now.saturating_since(d.opened_at) < settled_after
    })
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
