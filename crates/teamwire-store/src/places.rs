//! Where team databases live under `$TEAMWIRE_HOME`: each running team's
//! database under `teams/`, and each complete archive under `archive/`.
//! Readers that look across teams list them here.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use teamwire_core::TeamName;

use crate::Store;

/// One team database found under the home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamDb {
    pub path: PathBuf,
    /// A running team, not an archive.
    pub live: bool,
    /// For an archive: when its `SHA256SUMS` was written, so the newest of
    /// several archives of one team sorts last.
    pub rank: Option<SystemTime>,
}

/// Every running team's database, then every complete archive, each group
/// sorted by path. An entry that cannot be read goes to `errors` and the
/// rest are still listed.
pub fn team_databases(home: &Path, errors: &mut Vec<String>) -> Vec<TeamDb> {
    let mut list = Vec::new();
    for (dir, live) in [("teams", true), ("archive", false)] {
        let root = home.join(dir);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => paths.push(entry.path().join("team.db")),
                Err(e) => errors.push(format!("{}: {e}", root.display())),
            }
        }
        paths.sort();
        for path in paths.into_iter().filter(|p| p.is_file()) {
            // An archive without its checksum file is a copy that was cut
            // short; the team it holds is counted from elsewhere.
            let rank = if live {
                None
            } else {
                let sums = path.with_file_name("SHA256SUMS");
                match std::fs::metadata(&sums).and_then(|m| m.modified()) {
                    Ok(time) => Some(time),
                    Err(_) => continue,
                }
            };
            list.push(TeamDb { path, live, rank });
        }
    }
    list
}

/// The newest complete archive of `team`. The team row names the team; the
/// folder name cannot, since `a-2026` is a team and also the start of
/// `a-2026-2026-09-30`.
pub fn newest_archive(home: &Path, team: &TeamName, errors: &mut Vec<String>) -> Option<TeamDb> {
    let named = |db: &TeamDb| {
        Store::open_read_only_at(&db.path)
            .and_then(|store| store.read(|tx| Ok(tx.team()?.map(|t| t.name))))
            .is_ok_and(|name| name.is_some_and(|n| n == *team))
    };
    team_databases(home, errors)
        .into_iter()
        .filter(|db| !db.live && named(db))
        .max_by_key(|db| db.rank)
}
