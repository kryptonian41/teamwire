//! Deleting a closed team's state directory so that a kill at any moment
//! leaves either the whole directory, database included, or no directory.
//! `remove_dir_all` removes files one at a time, so a kill partway through
//! once left a `teams/<team>` with no `team.db`: nothing could read it, and a
//! rerun of close could not finish it. The directory is first renamed out of
//! `teams/` into `<home>/deleting/<team>.<pid>`, which is one atomic step on
//! one filesystem, and only then removed. A directory a killed close left
//! there is removed by a later close, once the pid in its name is dead.

use std::path::{Path, PathBuf};

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
    let doomed = stage(ctx, team.as_str(), team_dir)?;
    Ok(match std::fs::remove_dir_all(&doomed) {
        Ok(()) => format!("deleted {}", team_dir.display()),
        Err(e) => format!(
            "deleted {} from teams/; {} is left for a later close to remove: {e}",
            team_dir.display(),
            doomed.display()
        ),
    })
}

/// Moves `from` into `<home>/deleting/<name>.<pid>` and returns that path.
fn stage(ctx: &Ctx<'_>, name: &str, from: &Path) -> Result<PathBuf, String> {
    let deleting = ctx.home.join(DELETING);
    std::fs::create_dir_all(&deleting)
        .map_err(|e| format!("cannot create {}: {e}", deleting.display()))?;
    remove_left_over(ctx, &deleting);
    let doomed = deleting.join(format!("{name}.{}", std::process::id()));
    // A directory under this name was left by an earlier process with the
    // same pid, which is dead: this process now owns the pid.
    if doomed.exists() {
        std::fs::remove_dir_all(&doomed)
            .map_err(|e| format!("cannot remove {}: {e}", doomed.display()))?;
    }
    move_dir(from, &doomed)
        .map_err(|e| format!("cannot move {} to deleting/: {e}", from.display()))?;
    Ok(doomed)
}

/// The closer's clone and build folder for a team. Nothing else removes it,
/// and a build in it can hold tens of gigabytes.
pub(crate) fn closer_cache(ctx: &Ctx<'_>, team: &TeamName) -> PathBuf {
    ctx.home.join("cache").join("closer").join(team.as_str())
}

/// Moves the closer's cache folder into `deleting/` if there is one. The
/// caller does this before it deletes the team directory, so a kill between
/// the two leaves the folder where a later close of any team removes it.
/// A failure is only a note: the team's work is already safe in the archive.
pub(crate) fn stage_closer_cache(
    ctx: &Ctx<'_>,
    team: &TeamName,
) -> Option<Result<PathBuf, String>> {
    let dir = closer_cache(ctx, team);
    std::fs::symlink_metadata(&dir).ok()?;
    // `_` cannot occur in a team name, so this never collides with a team's
    // own staged directory.
    Some(stage(ctx, &format!("closer_{}", team.as_str()), &dir))
}

/// Removes a folder `stage_closer_cache` moved, and returns the note for the
/// step's detail.
pub(crate) fn remove_staged_cache(staged: Result<PathBuf, String>) -> String {
    match staged {
        Err(e) => format!("closer cache kept: {e}"),
        Ok(doomed) => match std::fs::remove_dir_all(&doomed) {
            Ok(()) => "deleted the closer cache".to_owned(),
            Err(e) => format!(
                "closer cache moved to {}, left for a later close to remove: {e}",
                doomed.display()
            ),
        },
    }
}

/// Removes every `<name>.<pid>` directory under `deleting` whose pid is
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
            tracing::warn!(path = %entry.path().display(), error = %e, "a left-over directory in deleting/ could not be removed");
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
