//! Where this crate reads and writes under the team directory.

use std::path::PathBuf;

use teamwire_core::Role;
use teamwire_store::Ctx;

/// `$TEAMWIRE_HOME/teams/<team>`, or `None` when the store has no team row.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub(crate) fn team_dir(ctx: &Ctx<'_>) -> Option<PathBuf> {
    let team = ctx.store.read(|tx| tx.team()).ok().flatten()?;
    Some(ctx.home.join("teams").join(team.name.as_str()))
}

/// The member's own report, named as the report check and the hold note name
/// it (`teamwire_verify::report_path`).
pub(crate) fn report_file(ctx: &Ctx<'_>, role: &Role) -> PathBuf {
    let dir = team_dir(ctx).unwrap_or_else(|| ctx.home.to_path_buf());
    teamwire_verify::report_path(&dir, role)
}
