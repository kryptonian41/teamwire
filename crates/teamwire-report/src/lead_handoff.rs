//! The lead's own handoff (ADR 0051): the note beside a tool's result once
//! the lead's context has passed its handoff point, and the one held stop
//! after it. Each is given once per session, recorded as an empty marker
//! file outside every store, because the lead's hooks never write a store
//! (opening one for a write could migrate an older database).
//!
//! Neither text urges haste: a model that thinks it is near its limit wraps
//! up early (see `handoff.rs`).

use std::path::Path;

use teamwire_core::{
    Fill, HandoffFiles, Harness, LeadHandoffSetting, Platform, SessionId, ToolCall,
};

use crate::lead_stop::led_by;

/// The note for the lead's tool call, once per session, when the session
/// leads an open team and its fill has reached its handoff point. A fill
/// that cannot be read, a failed marker write or a failed team listing
/// means no note: a hook never fails a turn.
pub fn lead_handoff_note(
    home: &Path,
    call: &ToolCall,
    harness: &dyn Harness,
    platform: &dyn Platform,
    setting: &LeadHandoffSetting,
) -> Option<String> {
    if setting.is_off() || call.subagent {
        return None;
    }
    let files = HandoffFiles::of(home, &call.session)?;
    // Every tool call of a lead comes here, so the cheapest check goes first.
    if files.noted.exists() || !leads_open_team(home, &call.session) {
        return None;
    }
    let fill = match harness.context_fill(&call.session, None) {
        Ok(fill) => fill?,
        Err(e) => {
            tracing::debug!(error = %e, "could not read the lead's context fill");
            return None;
        }
    };
    let point = fill.lead_handoff_at(setting.tokens());
    if fill.tokens < point {
        return None;
    }
    mark(platform, &files, &files.noted)?;
    Some(note_text(fill, point))
}

/// The line the lead's Stop hook adds to its hold, once per session, after
/// the note and before any handoff: `noted` exists, `held` does not, and the
/// session still leads an open team. The caller has already ruled out a
/// subagent's stop and a hold that is already active.
pub fn lead_handoff_hold(
    home: &Path,
    session: &SessionId,
    platform: &dyn Platform,
    setting: &LeadHandoffSetting,
) -> Option<String> {
    if setting.is_off() {
        return None;
    }
    let files = HandoffFiles::of(home, session)?;
    if !files.noted.exists() || files.held.exists() || !leads_open_team(home, session) {
        return None;
    }
    mark(platform, &files, &files.held)?;
    Some(HOLD_TEXT.to_owned())
}

const HOLD_TEXT: &str = "[teamwire] Your context passed its handoff point. If you are not waiting for the user's answer, run `teamwire lead --handoff-guide` now and follow it. If you are waiting for the user, stop again and hand off after the answer.";

/// Whether `session` leads an open team that could be read.
fn leads_open_team(home: &Path, session: &SessionId) -> bool {
    led_by(home, session)
        .iter()
        .any(|(_, read)| matches!(read, Ok(Some(_))))
}

/// Writes the empty marker `file`, making its folder first. `None`, logged,
/// when either write fails: the text is then not given, so it is never
/// given twice for want of a record.
fn mark(platform: &dyn Platform, files: &HandoffFiles, file: &Path) -> Option<()> {
    platform
        .create_dir_all(&files.dir)
        .and_then(|()| platform.write_atomic(file, b""))
        .map_err(|e| {
            tracing::warn!(error = %e, file = %file.display(), "could not record the lead handoff marker");
        })
        .ok()
}

fn note_text(fill: Fill, point: u64) -> String {
    format!(
        "[teamwire] Your context holds {}k tokens, {}% of a {}k window. Your handoff point is {}k. \
         Finish the step you are on. Then run `teamwire lead --handoff-guide` and follow it. \
         It has you write a handoff document and run `teamwire lead --handoff`, which starts a \
         fresh lead session beside you and gives it your teams. \
         If you are waiting for the user's answer, hand off after you have it.",
        fill.tokens / 1000,
        fill.pct(),
        fill.window / 1000,
        point / 1000,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_states_the_numbers_and_the_next_step() {
        let fill = Fill {
            tokens: 412_000,
            window: 1_000_000,
        };
        assert_eq!(
            note_text(fill, 400_000),
            "[teamwire] Your context holds 412k tokens, 41% of a 1000k window. Your handoff point is 400k. Finish the step you are on. Then run `teamwire lead --handoff-guide` and follow it. It has you write a handoff document and run `teamwire lead --handoff`, which starts a fresh lead session beside you and gives it your teams. If you are waiting for the user's answer, hand off after you have it."
        );
    }
}
