//! The sleep hold (filled in by the sleep-hold unit).

use teamwire_store::Ctx;

use crate::ValidPlan;

pub(crate) fn ensure_hold(_ctx: &mut Ctx<'_>, _plan: &ValidPlan, _warnings: &mut Vec<String>) {}
