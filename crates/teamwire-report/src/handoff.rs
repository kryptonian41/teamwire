//! The in-turn note that a member's context has passed its handoff point.
//! It states the numbers and the next step and does not urge haste: a model
//! that thinks it is near its limit wraps up early.

use std::path::Path;

use teamwire_core::{Fill, Harness, Role, SessionId};
use teamwire_store::Ctx;

use crate::paths::report_file;

/// The note for `me`, once per generation, when the session's fill has
/// reached its handoff point. A fill that cannot be read, a store error or a
/// note already given means no note: a hook never fails a turn.
pub fn handoff_note(
    ctx: &mut Ctx<'_>,
    me: &Role,
    harness: &dyn Harness,
    session: &SessionId,
    transcript: Option<&Path>,
) -> Option<String> {
    let fill = match harness.context_fill(session, transcript) {
        Ok(fill) => fill?,
        Err(e) => {
            tracing::debug!(error = %e, "could not read the context fill");
            return None;
        }
    };
    let (configured, generation) = ctx
        .store
        .read(|tx| {
            let generation = tx.member(me)?.map(|m| m.generation);
            Ok((tx.context_handoff_tokens(me)?, generation))
        })
        .map_err(|e| tracing::warn!(error = %e, "could not read the handoff point"))
        .ok()?;
    if !fill.past_handoff(configured) {
        return None;
    }
    let key = format!("handoff-note:{me}:{}", generation?);
    let at = ctx.clock.now();
    let first = ctx
        .store
        .write(|tx| tx.announce_once(at, &key))
        .map_err(|e| tracing::warn!(error = %e, "could not record the handoff note"))
        .ok()?;
    first.then(|| note(fill, fill.handoff_at(configured), &report_file(ctx, me)))
}

fn note(fill: Fill, point: u64, report: &Path) -> String {
    format!(
        "[teamwire] Your context holds {}k tokens, {}% of a {}k window. Your handoff point is {}k. \
         Finish the step you are on and commit it. Then run `teamwire handoff --guide` and follow it. \
         It has you write {} and record done with a summary that starts with \"handoff:\". \
         The lead then starts a fresh session of you that reads that file.",
        fill.tokens / 1000,
        fill.pct(),
        fill.window / 1000,
        point / 1000,
        report.with_extension("handoff.md").display(),
    )
}
