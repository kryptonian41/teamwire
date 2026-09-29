//! Typing one line into a located session.

use teamwire_core::{DeliveryError, DeliveryErrorCode, OneLine};
use teamwire_store::{Ctx, Party};

use crate::DeliveryOutcome;
use crate::locate::{locate_with, recheck};

/// Locates `party` and types `line` into its session.
pub fn deliver_line(ctx: &Ctx<'_>, party: &Party, line: &OneLine) -> DeliveryOutcome {
    let _ = line;
    let (harness, host) = match ctx.adapters(party) {
        Ok(pair) => pair,
        Err(e) => return DeliveryOutcome::Undelivered(e),
    };
    let found = match locate_with(ctx, party, harness, host) {
        Ok(found) => found,
        Err(e) => return DeliveryOutcome::Undelivered(e),
    };
    match recheck(host, &found) {
        Ok(()) => DeliveryOutcome::Undelivered(DeliveryError::new(
            DeliveryErrorCode::HostOther,
            "typing is not built yet",
        )),
        Err(e) => DeliveryOutcome::Undelivered(e),
    }
}
