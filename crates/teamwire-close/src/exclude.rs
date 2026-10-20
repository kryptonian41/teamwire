//! Taking back the `info/exclude` lines launch added, once the worktrees
//! that needed them are gone. Launch records each line it added in the
//! member's `member.started` data; a line that was already in the file is
//! not launch's and stays. `/TASK.md` is different: teamwire always owns it,
//! because a second team on the same repository finds it present and
//! records nothing, so it goes once no worktree needs it, whoever added it.
//!
//! Every read-modify-write holds the exclude lock launch also takes:
//! `$TEAMWIRE_HOME/locks/exclude-<sha256 of the path>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use teamwire_core::{EventType, Member, Mode, Pid, Platform, Role, exclude};
use teamwire_store::Ctx;

use crate::git;

/// The line every teamwire worktree's brief needs (LCH-24).
const TASK_MD: &str = "/TASK.md";

/// How often, and how long apart, close tries for another process's lock:
/// a holder only reads and rewrites one small file.
const LOCK_TRIES: u32 = 200;
const LOCK_PAUSE: Duration = Duration::from_millis(50);

/// Takes back, from each repository `members` worked in, every line launch
/// recorded for them and `/TASK.md`, unless a remaining linked worktree of
/// that repository holds the path the line names. Call it after the
/// worktrees are removed and pruned. Returns notes for the step output; a
/// line that cannot be taken back is a note, never a failed close, because
/// the team's work is safe either way.
pub(crate) fn take_back(ctx: &Ctx<'_>, members: &[&Member]) -> Vec<String> {
    // The exclude file, as its canonical path, to its repository and lines.
    let mut files: BTreeMap<PathBuf, (PathBuf, Vec<String>)> = BTreeMap::new();
    let mut notes = Vec::new();
    for member in members.iter().filter(|m| m.mode == Mode::Worktree) {
        let Some(repo) = &member.repo else {
            continue;
        };
        let mut add = |file: PathBuf, lines: Vec<String>| {
            let entry = files
                .entry(canonical(&file))
                .or_insert_with(|| (repo.clone(), Vec::new()));
            for line in lines {
                if !entry.1.contains(&line) {
                    entry.1.push(line);
                }
            }
        };
        // A repository git can no longer read has no exclude file left to fix.
        if let Ok(file) = exclude_path(ctx.platform, repo) {
            add(file, vec![TASK_MD.to_owned()]);
        }
        match recorded(ctx, &member.role) {
            Ok(records) => records.into_iter().for_each(|(f, l)| add(f, l)),
            Err(why) => notes.push(format!(
                "{}: the lines launch added to info/exclude are unknown, so only /TASK.md is taken back: {why}",
                member.role
            )),
        }
    }
    for (file, (repo, lines)) in files {
        notes.extend(take_back_file(ctx, &file, &repo, &lines));
    }
    notes
}

/// What launch recorded in the role's `member.started` data, from every
/// start (a reopen records again).
fn recorded(ctx: &Ctx<'_>, role: &Role) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
    let events = ctx
        .store
        .read(|tx| tx.events_of_role(role)?.all_or_corrupt())
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for event in events.iter().filter(|e| e.kind == EventType::MemberStarted) {
        let entries = event
            .data
            .get("excluded")
            .and_then(serde_json::Value::as_array);
        for entry in entries.into_iter().flatten() {
            let Some(file) = entry.get("file").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let lines = entry
                .get("lines")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect();
            out.push((PathBuf::from(file), lines));
        }
    }
    Ok(out)
}

/// Removes from one exclude file each of `lines` it holds that no remaining
/// worktree needs. The worktrees are listed under the lock, so a launch that
/// is adding a worktree now is either seen or adds its line back itself.
fn take_back_file(ctx: &Ctx<'_>, file: &Path, repo: &Path, lines: &[String]) -> Option<String> {
    let shown = file.display();
    let _lock = match lock_exclude(ctx.platform, ctx.home, file) {
        Ok(lock) => lock,
        Err(why) => return Some(format!("{why}; its lines are left in place")),
    };
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            return Some(format!(
                "could not read {shown} to take back launch's lines: {e}"
            ));
        }
    };
    let worktrees = match linked_worktrees(ctx.platform, repo) {
        Ok(w) => w,
        Err(why) => {
            return Some(format!(
                "launch's lines in {shown} are left in place: the remaining worktrees cannot be listed ({why})"
            ));
        }
    };
    let held: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    let gone: Vec<String> = lines
        .iter()
        .filter(|l| held.contains(&l.as_bytes()))
        .filter(|l| !needed(l, &worktrees))
        .cloned()
        .collect();
    if gone.is_empty() {
        return None;
    }
    match ctx
        .platform
        .write_atomic(file, &exclude::remove(&bytes, &gone))
    {
        Ok(()) => Some(format!("took back {} from {shown}", gone.join(", "))),
        Err(e) => Some(format!(
            "could not take back {} from {shown}: {e}",
            gone.join(", ")
        )),
    }
}

/// True when a worktree holds the path `line` names. A line whose path
/// cannot be read back is kept: close cannot tell who needs it.
fn needed(line: &str, worktrees: &[PathBuf]) -> bool {
    exclude::path_of(line).is_none_or(|path| {
        worktrees
            .iter()
            .any(|w| w.join(&path).symlink_metadata().is_ok())
    })
}

/// The repository's linked worktrees. The main checkout is the user's own:
/// launch never works there, so a file of the user's must not keep a line.
fn linked_worktrees(platform: &dyn Platform, repo: &Path) -> Result<Vec<PathBuf>, String> {
    let text = git::stdout_of(platform, repo, &["worktree", "list", "--porcelain"])?;
    Ok(text
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .skip(1)
        .map(PathBuf::from)
        .collect())
}

/// The repository's `info/exclude`, found by git.
fn exclude_path(platform: &dyn Platform, repo: &Path) -> Result<PathBuf, String> {
    let text = git::stdout_of(platform, repo, &["rev-parse", "--git-path", "info/exclude"])?;
    let path = PathBuf::from(text);
    Ok(if path.is_absolute() {
        path
    } else {
        repo.join(path)
    })
}

/// The path with its folder resolved, so one file reached two ways (a
/// symlinked temp directory, a relative git answer) is one key and one lock.
fn canonical(file: &Path) -> PathBuf {
    match (file.parent(), file.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(name),
        _ => file.to_path_buf(),
    }
}

/// A held lock on one `info/exclude`. Dropping it releases the lock.
struct ExcludeLock {
    dir: PathBuf,
}

impl Drop for ExcludeLock {
    fn drop(&mut self) {
        // A lock left here names this pid, so it is cleared once this process ends.
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            tracing::warn!(lock = %self.dir.display(), error = %e, "an exclude lock could not be released");
        }
    }
}

/// The lock launch takes too (teamwire-launch's worktree.rs): a directory
/// that `create_dir` makes in one atomic step, holding the owner's pid. A
/// lock whose owner is dead is removed; waiting is bounded.
fn lock_exclude(platform: &dyn Platform, home: &Path, file: &Path) -> Result<ExcludeLock, String> {
    let locks = home.join("locks");
    std::fs::create_dir_all(&locks).map_err(|e| format!("cannot make {}: {e}", locks.display()))?;
    let digest = Sha256::digest(canonical(file).to_string_lossy().as_bytes());
    let dir = locks.join(format!("exclude-{}", hex::encode(digest)));
    for _ in 0..LOCK_TRIES {
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                let lock = ExcludeLock { dir };
                let pid = std::process::id().to_string();
                platform
                    .write_atomic(&lock.dir.join("pid"), pid.as_bytes())
                    .map_err(|e| format!("cannot write the lock {}: {e}", lock.dir.display()))?;
                return Ok(lock);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if holder_is_dead(platform, &dir) {
                    std::fs::remove_dir_all(&dir).ok();
                } else {
                    std::thread::sleep(LOCK_PAUSE);
                }
            }
            Err(e) => return Err(format!("cannot make the lock {}: {e}", dir.display())),
        }
    }
    Err(format!(
        "another teamwire process held the lock {} on {} for {} s",
        dir.display(),
        file.display(),
        (LOCK_PAUSE * LOCK_TRIES).as_secs()
    ))
}

/// True when the lock's pid file names a process that is not running. A
/// pid of this process is another thread's hold, which is live.
fn holder_is_dead(platform: &dyn Platform, dir: &Path) -> bool {
    // shortcut: a lock with no pid file counts as held, because its owner may
    // be between `create_dir` and the pid write; a process killed in that gap
    // leaves a lock someone must delete. Upgrade if that is ever seen.
    let Ok(text) = std::fs::read_to_string(dir.join("pid")) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    pid != std::process::id() && !platform.pid_alive(Pid::new(pid))
}
