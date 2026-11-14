//! Making and taking back a member's git worktree (LCH-8, LCH-24). git is
//! always run through `Platform::run`, and the host's own worktree command
//! is never used (INV-23). Every change to the repository's shared
//! `info/exclude` holds the exclude lock that close also takes, and every
//! `git worktree add` or `remove` holds the worktree lock close also takes.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use teamwire_core::{Platform, exclude};

use crate::WorktreePlan;
use crate::git::{failure_text, run, stdout_of};
use crate::worktree_lock::lock_worktrees;

/// What launch made, so a failure can take exactly that back.
#[derive(Debug, Clone)]
pub(crate) struct Made {
    pub(crate) repo: PathBuf,
    pub(crate) path: PathBuf,
    /// Set when launch created the branch (and so may delete it again).
    pub(crate) new_branch: Option<String>,
}

/// The `info/exclude` lines one launch added, so a failed launch can take
/// exactly those back and leave every earlier line.
#[derive(Debug, Clone)]
pub(crate) struct Excluded {
    pub(crate) file: PathBuf,
    pub(crate) lines: Vec<String>,
}

/// The commit the member starts from: the branch's tip when the branch
/// exists (the base is ignored then), else the base, else `HEAD`.
pub(crate) fn start_commit(platform: &dyn Platform, w: &WorktreePlan) -> Result<String, String> {
    let spec = if w.branch_exists {
        format!("refs/heads/{}^{{commit}}", w.branch)
    } else {
        format!("{}^{{commit}}", w.base.as_deref().unwrap_or("HEAD"))
    };
    let sha = stdout_of(platform, &w.repo, &["rev-parse", "--verify", &spec])?;
    if sha.is_empty() {
        return Err(format!("git could not name the start commit {spec}"));
    }
    Ok(sha)
}

/// `git worktree add`, at the recorded start commit, under the worktree
/// lock.
pub(crate) fn add(
    platform: &dyn Platform,
    home: &Path,
    w: &WorktreePlan,
    sha: &str,
) -> Result<Made, String> {
    if w.reuse {
        return Ok(Made {
            repo: w.repo.clone(),
            path: w.path.clone(),
            new_branch: None,
        });
    }
    if let Some(parent) = w.path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    let path = w.path.display().to_string();
    let args: Vec<&str> = if w.branch_exists {
        vec!["worktree", "add", &path, &w.branch]
    } else {
        vec!["worktree", "add", "-b", &w.branch, &path, sha]
    };
    let out = {
        let _lock = lock_worktrees(platform, home, &w.repo)?;
        run(platform, &w.repo, &args)?
    };
    if !out.success() {
        return Err(format!("git worktree add failed: {}", failure_text(&out)));
    }
    Ok(Made {
        repo: w.repo.clone(),
        path: w.path.clone(),
        new_branch: (!w.branch_exists).then(|| w.branch.clone()),
    })
}

/// Takes the worktree back without force, so uncommitted work is never
/// thrown away, under the worktree lock. Returns what happened, in words,
/// whether the worktree is still there, and whether the branch launch
/// created was deleted.
pub(crate) fn remove(platform: &dyn Platform, home: &Path, made: &Made) -> (String, bool, bool) {
    let path = made.path.display().to_string();
    let mut left = true;
    let removed = lock_worktrees(platform, home, &made.repo)
        .and_then(|_lock| run(platform, &made.repo, &["worktree", "remove", &path]));
    let mut note = match removed {
        Ok(out) if out.success() => {
            left = false;
            format!("worktree {path} removed")
        }
        Ok(out) => format!(
            "could not remove worktree {path}: {}{}",
            failure_text(&out),
            leftovers(platform, &made.path)
        ),
        Err(e) => format!("could not remove worktree {path}: {e}"),
    };
    let mut deleted = false;
    if let Some(branch) = &made.new_branch {
        // `-d` refuses a branch with commits that are not merged, which keeps
        // any work a member managed to commit.
        match run(platform, &made.repo, &["branch", "-d", branch]) {
            Ok(out) if out.success() => {
                deleted = true;
                note = format!("{note}; branch {branch} deleted");
            }
            _ => note = format!("{note}; branch {branch} left in place"),
        }
    }
    (note, left, deleted)
}

/// The paths a failed removal left, from `git status`, and the step that
/// clears them. git refuses to remove a worktree that holds such files, and
/// launch never forces it.
fn leftovers(platform: &dyn Platform, path: &Path) -> String {
    // Raw output: a trimmed first line ` M Cargo.lock` would lose its space
    // and so the first letter of its path.
    let Ok(out) = run(
        platform,
        path,
        &["status", "--porcelain", "--untracked-files=all"],
    ) else {
        return String::new();
    };
    if !out.success() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let paths: Vec<&str> = text.lines().filter_map(|l| l.get(3..)).collect();
    if paths.is_empty() {
        return String::new();
    }
    let shown = paths
        .iter()
        .take(10)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let more = paths.len().saturating_sub(10);
    let more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!(
        "; it still holds {shown}{more}. Save or delete those files, then run \
         `git worktree remove {}`",
        path.display()
    )
}

/// The repository's `info/exclude` for this worktree, found by git.
fn exclude_path(platform: &dyn Platform, worktree: &Path) -> Result<PathBuf, String> {
    let text = stdout_of(
        platform,
        worktree,
        &["rev-parse", "--git-path", "info/exclude"],
    )?;
    let path = PathBuf::from(text);
    Ok(if path.is_absolute() {
        path
    } else {
        worktree.join(path)
    })
}

/// Adds `/TASK.md` to the repository's `info/exclude` (LCH-24), once. The
/// line is anchored, so a `TASK.md` deeper in the user's checkout still shows
/// in `git status`.
pub(crate) fn exclude_task_md(
    platform: &dyn Platform,
    home: &Path,
    worktree: &Path,
) -> Result<Option<Excluded>, String> {
    let (file, lines) = exclude_lines(platform, home, worktree, &["/TASK.md".to_owned()])?;
    Ok((!lines.is_empty()).then_some(Excluded { file, lines }))
}

/// How often, and how long apart, launch tries for another process's lock
/// on an `info/exclude`: a holder only reads and rewrites one small file, so
/// ten seconds is far past any real hold.
const LOCK_TRIES: u32 = 200;
const LOCK_PAUSE: Duration = Duration::from_millis(50);

/// A held lock on one `info/exclude`. Dropping it closes the file, which
/// releases the lock.
struct ExcludeLock {
    _file: File,
}

/// Takes the lock every read-modify-write of `file` holds, so two launches
/// (or a launch and a close) on one repository never drop each other's
/// lines: the kernel's `flock` on `$TEAMWIRE_HOME/locks/exclude-<sha256 of
/// the path>.lock`, a file made if missing and never deleted. The lock ends
/// with the open file, so a holder that is killed leaves nothing held, and
/// each open file waits for the others, threads of one process too. Close
/// takes the same lock.
#[expect(
    clippy::disallowed_methods,
    reason = "the exclude lock file is only ever locked, never written"
)]
fn lock_exclude(home: &Path, file: &Path) -> Result<ExcludeLock, String> {
    let (Some(parent), Some(name)) = (file.parent(), file.file_name()) else {
        return Err(format!("{} has no folder", file.display()));
    };
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    // The same file reached through a symlink must get the same lock.
    let key = std::fs::canonicalize(parent)
        .unwrap_or_else(|_| parent.to_path_buf())
        .join(name);
    let locks = home.join("locks");
    std::fs::create_dir_all(&locks).map_err(|e| format!("cannot make {}: {e}", locks.display()))?;
    let digest = Sha256::digest(key.to_string_lossy().as_bytes());
    let path = locks.join(format!("exclude-{}.lock", hex::encode(digest)));
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("cannot open the lock {}: {e}", path.display()))?;
    for _ in 0..LOCK_TRIES {
        match lock.try_lock() {
            Ok(()) => return Ok(ExcludeLock { _file: lock }),
            Err(TryLockError::WouldBlock) => std::thread::sleep(LOCK_PAUSE),
            Err(TryLockError::Error(e)) => {
                return Err(format!("cannot take the lock {}: {e}", path.display()));
            }
        }
    }
    Err(format!(
        "another teamwire process held the lock {} on {} for {} s; try again",
        path.display(),
        file.display(),
        (LOCK_PAUSE * LOCK_TRIES).as_secs()
    ))
}

/// Appends each entry to the repository's `info/exclude` unless a line
/// already names it, under the exclude lock. Returns the file and the
/// entries that were added.
fn exclude_lines(
    platform: &dyn Platform,
    home: &Path,
    worktree: &Path,
    entries: &[String],
) -> Result<(PathBuf, Vec<String>), String> {
    let path = exclude_path(platform, worktree)?;
    let _lock = lock_exclude(home, &path)?;
    let added = append_locked(platform, &path, entries)?;
    Ok((path, added))
}

/// `exclude_lines` for a caller that already holds the exclude lock on
/// `path`. Returns the entries that were added.
fn append_locked(
    platform: &dyn Platform,
    path: &Path,
    entries: &[String],
) -> Result<Vec<String>, String> {
    // Only a missing file is empty. Any other read error must not turn into
    // a fresh file that replaces the repository's shared rules.
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let (bytes, added) = exclude::add(&bytes, entries);
    if !added.is_empty() {
        platform
            .write_atomic(path, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(added)
}

/// Removes the lines `excluded` added, one occurrence each, under the
/// exclude lock; every other line stays as it was. Returns a note for the
/// undo output.
pub(crate) fn unexclude(platform: &dyn Platform, home: &Path, excluded: &Excluded) -> String {
    let file = excluded.file.display();
    let _lock = match lock_exclude(home, &excluded.file) {
        Ok(lock) => lock,
        Err(e) => return format!("could not take back the lines launch added to {file}: {e}"),
    };
    let bytes = match std::fs::read(&excluded.file) {
        Ok(bytes) => bytes,
        Err(e) => return format!("could not read {file} to take back its added lines: {e}"),
    };
    let kept = exclude::remove(&bytes, &excluded.lines);
    match platform.write_atomic(&excluded.file, &kept) {
        Ok(()) => format!("lines launch added to {file} removed"),
        Err(e) => format!("could not take back the lines launch added to {file}: {e}"),
    }
}

/// Tells git to ignore what a member's setup command created: every
/// untracked, not-ignored path of the worktree, anchored to the repository
/// root. Without this, `teamwire close` finds the files and refuses the
/// worktree as holding uncommitted work. Returns notes for the launch output
/// naming every path added or skipped; a failure is a note, not a launch
/// failure, because the member can still work. Also returns the lines it
/// added, for the undo.
pub(crate) fn exclude_setup_files(
    platform: &dyn Platform,
    home: &Path,
    worktree: &Path,
) -> (Vec<String>, Option<Excluded>) {
    let file = match exclude_path(platform, worktree) {
        Ok(file) => file,
        Err(e) => {
            return (
                vec![format!("setup created files git does not ignore: {e}")],
                None,
            );
        }
    };
    // The listing runs under the lock: it reads the exclude file, and a close
    // that takes a line back between the listing and the append would leave
    // a file setup made neither listed nor ignored.
    let _lock = match lock_exclude(home, &file) {
        Ok(lock) => lock,
        Err(e) => {
            return (
                vec![format!("setup created files git does not ignore: {e}")],
                None,
            );
        }
    };
    // One entry per file, never per directory: a directory line would also hide
    // files a member writes there later, and close would then delete them.
    // `-z` gives raw names, so quote-escaping never reaches the exclude file.
    let listed = run(
        platform,
        worktree,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    );
    let out = match listed {
        Ok(out) if out.success() => out,
        Ok(out) => return (vec![list_failed(&failure_text(&out))], None),
        Err(e) => return (vec![list_failed(&e)], None),
    };
    let mut notes = Vec::new();
    let mut entries = Vec::new();
    for raw in out.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        match std::str::from_utf8(raw) {
            Ok("TASK.md") => {}
            // A newline would split into two rules, so such a name is never written.
            Ok(p) if !p.contains('\n') => entries.push(exclude::anchored(p)),
            _ => notes.push(format!(
                "setup created {:?}, whose name cannot be written to the exclude file, so git does not ignore it",
                String::from_utf8_lossy(raw)
            )),
        }
    }
    if entries.is_empty() {
        return (notes, None);
    }
    let mut excluded = None;
    match append_locked(platform, &file, &entries) {
        Ok(added) if added.is_empty() => {}
        Ok(added) => {
            let them = if added.len() == 1 { "it" } else { "them" };
            notes.push(format!(
                "setup created {}; launch added {them} to {} so git ignores {them} (every worktree of the repository shares that file)",
                added.join(", "),
                file.display()
            ));
            excluded = Some(Excluded { file, lines: added });
        }
        Err(e) => notes.push(format!("setup created files git does not ignore: {e}")),
    }
    (notes, excluded)
}

fn list_failed(why: &str) -> String {
    format!("could not list what setup created, so none of it is ignored: {why}")
}

#[cfg(test)]
mod tests {
    use teamwire_fakes::FakePlatform;

    use super::{append_locked, lock_exclude};

    #[test]
    fn eight_writers_through_the_lock_keep_every_line_exactly_once() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let file = repo.path().join("info/exclude");
        let platform = FakePlatform::new();
        let line = |t: u32, round: u32| format!("/t{t}-r{round}");
        std::thread::scope(|s| {
            for t in 0..8 {
                let (home, file, platform) = (home.path(), &file, &platform);
                s.spawn(move || {
                    for round in 0..5 {
                        let _lock = lock_exclude(home, file).unwrap();
                        append_locked(platform, file, &[line(t, round)]).unwrap();
                    }
                });
            }
        });
        let text = std::fs::read_to_string(&file).unwrap();
        for t in 0..8 {
            for round in 0..5 {
                let want = line(t, round);
                let count = text.lines().filter(|l| *l == want).count();
                assert_eq!(count, 1, "{want} in {text}");
            }
        }
        assert_eq!(text.lines().count(), 40, "{text}");
    }
}
