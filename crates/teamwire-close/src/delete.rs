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
        .map_err(|e| format!("cannot move {} out of teams/: {e}", from.display()))?;
    Ok(doomed)
}

/// The closer's clone and build folder for a team. Nothing else removes it,
/// and a build in it can hold tens of gigabytes.
pub(crate) fn closer_cache(ctx: &Ctx<'_>, team: &TeamName) -> PathBuf {
    ctx.home.join("cache").join("closer").join(team.as_str())
}

/// Deletes the closer's cache folder if there is one. Returns a note for the
/// step's detail, or nothing when there was no folder. A failure is only a
/// note: the team's work is already safe in the archive.
pub(crate) fn delete_closer_cache(ctx: &Ctx<'_>, team: &TeamName) -> Option<String> {
    let dir = closer_cache(ctx, team);
    let meta = std::fs::symlink_metadata(&dir).ok()?;
    // A link is removed as a link: renaming then `remove_dir_all` could walk
    // into whatever it points at.
    if meta.file_type().is_symlink() {
        return Some(match std::fs::remove_file(&dir) {
            Ok(()) => format!("deleted the link {}", dir.display()),
            Err(e) => format!("cannot remove the link {}: {e}", dir.display()),
        });
    }
    let doomed = match stage(ctx, &format!("closer-{}", team.as_str()), &dir) {
        Ok(doomed) => doomed,
        Err(e) => return Some(format!("closer cache kept: {e}")),
    };
    Some(match std::fs::remove_dir_all(&doomed) {
        Ok(()) => format!("deleted {}", dir.display()),
        Err(e) => format!(
            "deleted {}; {} is left for a later close to remove: {e}",
            dir.display(),
            doomed.display()
        ),
    })
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
