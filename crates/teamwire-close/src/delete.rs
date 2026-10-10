//! Deleting a closed team's state directory so that a kill at any moment
//! leaves either the whole directory, database included, or no directory.
//! `remove_dir_all` removes files one at a time, so a kill partway through
//! once left a `teams/<team>` with no `team.db`: nothing could read it, and a
//! rerun of close could not finish it. The directory is first renamed out of
//! `teams/` into `<home>/deleting/<team>.<pid>`, which is one atomic step on
//! one filesystem, and only then removed. A directory a killed close left
//! there is removed by a later close, once the pid in its name is dead.

use std::path::Path;

use teamwire_core::{Pid, TeamName};
use teamwire_store::Ctx;

/// The directory beside `teams/` that holds team directories on their way
/// out. Nothing reads it as teams.
const DELETING: &str = "deleting";

/// Moves `team_dir` out of `teams/`, then removes it. The step fails only
/// when the move fails, because until then the team is still whole. A
/// removal that fails after the move leaves a directory the next close
/// removes, and the team itself is gone, so the step still succeeds.
pub(crate) fn delete_team_dir(
    ctx: &Ctx<'_>,
    team: &TeamName,
    team_dir: &Path,
) -> Result<String, String> {
    let deleting = ctx.home.join(DELETING);
    std::fs::create_dir_all(&deleting)
        .map_err(|e| format!("cannot create {}: {e}", deleting.display()))?;
    remove_left_over(ctx, &deleting);
    let doomed = deleting.join(format!("{}.{}", team.as_str(), std::process::id()));
    // A directory under this name was left by an earlier process with the
    // same pid, which is dead: this process now owns the pid.
    if doomed.exists() {
        std::fs::remove_dir_all(&doomed)
            .map_err(|e| format!("cannot remove {}: {e}", doomed.display()))?;
    }
    move_dir(team_dir, &doomed)
        .map_err(|e| format!("cannot move {} out of teams/: {e}", team_dir.display()))?;
    Ok(match std::fs::remove_dir_all(&doomed) {
        Ok(()) => format!("deleted {}", team_dir.display()),
        Err(e) => format!(
            "deleted {} from teams/; {} is left for a later close to remove: {e}",
            team_dir.display(),
            doomed.display()
        ),
    })
}

/// Removes every `<team>.<pid>` directory under `deleting` whose pid is
/// dead. A live pid is a close that is removing its own directory now.
fn remove_left_over(ctx: &Ctx<'_>, deleting: &Path) {
    let Ok(entries) = std::fs::read_dir(deleting) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.rsplit_once('.'))
            .and_then(|(_, pid)| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if ctx.platform.pid_alive(Pid::new(pid)) {
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(entry.path()) {
            tracing::warn!(path = %entry.path().display(), error = %e, "a left-over team directory could not be removed");
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "renaming a whole directory writes no file; Platform::write_atomic is for file contents"
)]
fn move_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}
