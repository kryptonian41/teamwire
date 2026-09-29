//! The per-member sequence (filled in by a later unit).

use teamwire_store::Ctx;

use crate::{LaunchOutcome, LaunchReport, MemberOutcome, ValidPlan};

pub(crate) fn launch_all(_ctx: &mut Ctx<'_>, plan: ValidPlan) -> LaunchReport {
    LaunchReport {
        members: plan
            .spec
            .children
            .iter()
            .map(|c| MemberOutcome {
                role: c.role.clone(),
                outcome: LaunchOutcome::Failed("not built".to_owned()),
            })
            .collect(),
        warnings: plan.warnings,
        watch_command: crate::watch_command(&plan.spec.team),
    }
}
