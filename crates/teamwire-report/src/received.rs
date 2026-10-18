//! `teamwire received` (ADR 0024): the lead says it read a member's result
//! from its file, after the typing failed or stalled.

use serde::{Deserialize, Serialize};
use teamwire_core::{ReportState, Role};
use teamwire_store::{Ctx, StoreError};

/// What `received` did for one role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Received {
    /// The member's newest result, number `n`, is now `delivered`.
    Marked { n: u32 },
    /// The member's newest result is not `undelivered` or `unconfirmed`, or
    /// it has none. `state` is that result's state.
    NotWaiting { state: Option<ReportState> },
    /// No member has this role.
    NoMember,
}

/// Marks each role's newest result received. Only the newest counts: an
/// older one was replaced, and its file overwritten, by the newer one.
pub fn mark_received(
    ctx: &mut Ctx<'_>,
    roles: &[Role],
) -> Result<Vec<(Role, Received)>, StoreError> {
    let at = ctx.clock.now();
    ctx.store.write(|tx| {
        let mut out = Vec::new();
        for role in roles {
            let outcome = if tx.reader().member(role)?.is_none() {
                Received::NoMember
            } else {
                match tx.reader().latest_completion(role)? {
                    Some(r) if tx.mark_result_received(at, &r.completion.ulid)? => {
                        Received::Marked { n: r.completion.n }
                    }
                    newest => Received::NotWaiting {
                        state: newest.map(|r| r.completion.report_state),
                    },
                }
            };
            out.push((role.clone(), outcome));
        }
        Ok(out)
    })
}
