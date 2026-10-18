//! A team whose store holds nothing: `teamwire launch` was killed after the
//! database file was made and before the team row was written. Every close
//! step needs a team row (the close lock, the archive), so this path takes
//! neither and only removes the directory.
//!
//! Never: run when the store holds a team row or any member row. The check
//! is made again right before the delete, because no lock guards it.

use std::path::PathBuf;

use teamwire_core::TeamName;
use teamwire_store::Ctx;

use crate::{CloseOpts, CloseStep, StepOutcome, delete};

/// The team and its directory, when the store has no team row, no member row
/// and no damaged row, and the caller named the team. A store that cannot be
/// read is not empty: the ordinary refusal explains it.
pub(crate) fn nothing_recorded(ctx: &Ctx<'_>, opts: &CloseOpts) -> Option<(TeamName, PathBuf)> {
    let team = opts.team.clone()?;
    let (row, members) = ctx.store.read(|tx| Ok((tx.team()?, tx.members()?))).ok()?;
    if row.is_some() || !members.items.is_empty() || !members.bad.is_empty() {
        return None;
    }
    let dir = ctx.home.join("teams").join(team.as_str());
    Some((team, dir))
}

/// The plan's one step. It reads as the step's output line, `<team>: ...`.
pub(crate) fn plan(team: &TeamName, dir: &std::path::Path) -> CloseStep {
    CloseStep {
        step: team.to_string(),
        detail: format!("nothing was recorded; deleted {}", dir.display()),
    }
}

/// Deletes the directory of a store that holds nothing.
pub(crate) fn run(ctx: &Ctx<'_>, team: &TeamName, dir: &std::path::Path) -> StepOutcome {
    let result = delete::delete_team_dir(ctx, team, dir);
    StepOutcome {
        step: team.to_string(),
        ok: result.is_ok(),
        detail: match result {
            Ok(detail) => format!("nothing was recorded; {detail}"),
            Err(detail) => detail,
        },
    }
}
