//! Which teams worked in the same repository as a caller. The MCP tool reads
//! only such teams, because an archive may hold another company's work. Plain
//! path code: no `git` command runs.

use std::path::{Component, Path, PathBuf};

use teamwire_core::{Member, Ulid};

/// What a caller may read: the teams that share a repository with it.
#[derive(Debug, Clone)]
pub struct RepoScope {
    /// The caller's own team, always readable.
    pub own: Ulid,
    /// The caller's directories (see [`repo_dirs`]).
    pub dirs: Vec<PathBuf>,
    /// The user's home directory. A member rooted there shares nothing.
    pub user_home: Option<PathBuf>,
}

impl RepoScope {
    /// The caller's team is `own`, with these members (removed ones too).
    pub fn new(own: Ulid, members: &[Member], user_home: Option<PathBuf>) -> Self {
        let dirs = repo_dirs(members, user_home.as_deref());
        Self {
            own,
            dirs,
            user_home,
        }
    }

    pub(crate) fn allows(&self, team: &Ulid, members: &[Member]) -> bool {
        if *team == self.own {
            return true;
        }
        let theirs = repo_dirs(members, self.user_home.as_deref());
        self.dirs
            .iter()
            .any(|a| theirs.iter().any(|b| a.starts_with(b) || b.starts_with(a)))
    }
}

/// Each member's `repo`, or its `cwd` when `repo` is empty, normalized: a
/// path under `<x>/.worktrees/...` becomes `<x>`. The user's home directory
/// and `/` are left out, since a member rooted there would match every team.
pub fn repo_dirs(members: &[Member], user_home: Option<&Path>) -> Vec<PathBuf> {
    let user_home = user_home.map(normalize);
    members
        .iter()
        .map(|m| normalize(m.repo.as_deref().unwrap_or(&m.cwd)))
        .filter(|d| d.parent().is_some() && user_home.as_ref() != Some(d))
        .collect()
}

/// The real path when the directory still exists, else the path with `.` and
/// `..` resolved by text; then cut at a `.worktrees` folder.
fn normalize(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| {
        let mut out = PathBuf::new();
        for part in path.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                other @ (Component::Prefix(_) | Component::RootDir | Component::Normal(_)) => {
                    out.push(other);
                }
            }
        }
        out
    });
    let mut kept = PathBuf::new();
    for part in resolved.components() {
        if part.as_os_str() == ".worktrees" {
            break;
        }
        kept.push(part);
    }
    kept
}

#[cfg(test)]
mod tests;
