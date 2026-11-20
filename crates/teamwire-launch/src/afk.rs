//! AFK teams in launch (the team row's `afk_budget_ends_at` is set): a spent
//! budget turns away new implementers, and a member below the AFK model
//! table gets a warning. Neither check applies to an ordinary team.

use teamwire_core::{Clock, MemberKind, Timestamp};
use teamwire_store::Store;

use crate::PlannedMember;
use crate::validate::Existing;

const MODELS: [&str; 3] = ["haiku", "sonnet", "opus"];
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The model and effort a member of `kind` should run on in an AFK run.
fn table(kind: MemberKind) -> (&'static str, &'static str) {
    match kind {
        MemberKind::Implementer | MemberKind::Investigator => ("opus", "high"),
        MemberKind::Reviewer
        | MemberKind::Closer
        | MemberKind::PlanReviewer
        | MemberKind::Reconciler => ("opus", "xhigh"),
    }
}

/// True when `have` is missing, or ranks below `want` in `order`. A name
/// outside the order is never below: the table cannot judge it.
fn below(order: &[&str], have: Option<&str>, want: &str) -> bool {
    let Some(have) = have else { return true };
    let (Some(h), Some(w)) = (
        order.iter().position(|x| *x == have),
        order.iter().position(|x| *x == want),
    ) else {
        return false;
    };
    h < w
}

/// One warning per member of an AFK team that runs below the AFK table.
pub(crate) fn model_warnings(
    existing: Option<&Existing>,
    members: &[PlannedMember],
    warnings: &mut Vec<String>,
) {
    if existing.is_none_or(|e| e.team.afk_budget_ends_at.is_none()) {
        return;
    }
    for m in members {
        let (model, effort) = table(m.kind);
        let (have_model, have_effort) = (m.spec.model.as_deref(), m.spec.effort.as_deref());
        if below(&MODELS, have_model, model) || below(&EFFORTS, have_effort, effort) {
            warnings.push(format!(
                "AFK run: member {} ({}) runs on {} at {}, below the AFK table ({model} at {effort})",
                m.spec.role,
                m.kind,
                have_model.unwrap_or("the default model"),
                have_effort.unwrap_or("the default effort"),
            ));
        }
    }
}

/// The refusal for a new implementer once the run's budget has ended.
/// Read from the team row at launch time, so a long `--add` sequence sees
/// the budget run out.
pub(crate) fn budget_refusal(
    store: &Store,
    clock: &dyn Clock,
    m: &PlannedMember,
) -> Option<String> {
    if m.kind != MemberKind::Implementer {
        return None;
    }
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    let ends = store.read(|tx| tx.team()).ok()??.afk_budget_ends_at?;
    let now: Timestamp = clock.now();
    (now >= ends).then(|| {
        format!("afk_budget_spent: the run's budget ended at {ends}; no new implementer starts")
    })
}
