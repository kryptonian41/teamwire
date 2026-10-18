//! A store with no team row, no member row and no message row: `teamwire
//! launch` was killed after the database file was made and before the team
//! row was written. Every close step needs a team row (the close lock, the
//! archive), so this path takes neither and only removes the directory.
//!
//! Event rows do not stop it: an older close that refused this same store
//! wrote a `close.refused` event.
//!
//! Never: run when the store holds a team row, a member row or a message
//! row. The check is made again right before the delete.

use std::path::PathBuf;

use teamwire_core::TeamName;
use teamwire_store::{Ctx, MessageFilter};

use crate::{CloseOpts, CloseStep, StepOutcome, delete};

/// The team and its directory, when the store has no team row, no member row,
/// no message row and no damaged row of either, and the caller named the
/// team. A store that cannot be read is not empty: the ordinary refusal
/// explains it.
pub(crate) fn nothing_recorded(ctx: &Ctx<'_>, opts: &CloseOpts) -> Option<(TeamName, PathBuf)> {
    let team = opts.team.clone()?;
    let (row, members, messages) = ctx
        .store
        .read(|tx| {
            Ok((
                tx.team()?,
                tx.members()?,
                tx.messages(&MessageFilter::default())?,
            ))
        })
        .ok()?;
    let rows = members.items.len() + members.bad.len() + messages.items.len() + messages.bad.len();
    if row.is_some() || rows > 0 {
        return None;
    }
    let dir = ctx.home.join("teams").join(team.as_str());
    Some((team, dir))
}

/// The plan's one step. It reads as the step's output line, `<team>: ...`.
pub(crate) fn plan(team: &TeamName, dir: &std::path::Path) -> CloseStep {
    CloseStep {
        step: team.to_string(),
        detail: format!(
            "no team or member was recorded; would delete {}",
            dir.display()
        ),
    }
}

/// Deletes the directory of a store with no team, member or message row.
pub(crate) fn run(ctx: &Ctx<'_>, team: &TeamName, dir: &std::path::Path) -> StepOutcome {
    // shortcut: no lock guards this delete, so a launch of the same team name
    // that writes its team row between the check and the rename loses its
    // store; upgrade if launch and close of one team name ever run together.
    let result = delete::delete_team_dir(ctx, team, dir);
    StepOutcome {
        step: team.to_string(),
        ok: result.is_ok(),
        detail: match result {
            Ok(detail) => format!("no team or member was recorded; {detail}"),
            Err(detail) => detail,
        },
    }
}
