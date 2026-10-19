//! Making and taking back a member's git worktree (LCH-8, LCH-24). git is
//! always run through `Platform::run`, and the host's own worktree command
//! is never used (INV-23).

use std::path::{Path, PathBuf};

use teamwire_core::Platform;

use crate::WorktreePlan;
use crate::git::{failure_text, run, stdout_of};

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

/// `git worktree add`, at the recorded start commit.
pub(crate) fn add(platform: &dyn Platform, w: &WorktreePlan, sha: &str) -> Result<Made, String> {
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
    let out = run(platform, &w.repo, &args)?;
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
/// thrown away. Returns what happened, in words, whether the worktree is
/// still there, and whether the branch launch created was deleted.
pub(crate) fn remove(platform: &dyn Platform, made: &Made) -> (String, bool, bool) {
    let path = made.path.display().to_string();
    let mut left = true;
    let mut note = match run(platform, &made.repo, &["worktree", "remove", &path]) {
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
    worktree: &Path,
) -> Result<Option<Excluded>, String> {
    let (file, lines) = exclude_lines(platform, worktree, &["/TASK.md".to_owned()])?;
    Ok((!lines.is_empty()).then_some(Excluded { file, lines }))
}

/// Appends each entry to the repository's `info/exclude` unless a line
/// already names it (with or without a leading `/`). Existing lines are never
/// rewritten or reordered. Returns the file and the entries that were added.
fn exclude_lines(
    platform: &dyn Platform,
    worktree: &Path,
    entries: &[String],
) -> Result<(PathBuf, Vec<String>), String> {
    let path = exclude_path(platform, worktree)?;
    // Only a missing file is empty. Any other read error, or text that is not
    // UTF-8, must not turn into a fresh file that replaces the repository's
    // shared rules.
    let mut bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut added: Vec<String> = Vec::new();
    let mut tail = Vec::new();
    for entry in entries {
        let bare = entry.strip_prefix('/').unwrap_or(entry).as_bytes();
        let present = bytes.split(|b| *b == b'\n').any(|l| {
            let l = l.trim_ascii();
            l.strip_prefix(b"/").unwrap_or(l) == bare
        });
        if !present && !added.contains(entry) {
            tail.extend_from_slice(entry.as_bytes());
            tail.push(b'\n');
            added.push(entry.clone());
        }
    }
    if added.is_empty() {
        return Ok((path, added));
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(&tail);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    platform
        .write_atomic(&path, &bytes)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok((path, added))
}

/// Removes the lines `excluded` added, one occurrence each; every other line
/// stays as it was. Returns a note for the undo output.
pub(crate) fn unexclude(platform: &dyn Platform, excluded: &Excluded) -> String {
    let file = excluded.file.display();
    let bytes = match std::fs::read(&excluded.file) {
        Ok(bytes) => bytes,
        Err(e) => return format!("could not read {file} to take back its added lines: {e}"),
    };
    let mut pending: Vec<&[u8]> = excluded.lines.iter().map(String::as_bytes).collect();
    let mut kept: Vec<&[u8]> = Vec::new();
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        let bare = line.strip_suffix(b"\n").unwrap_or(line);
        match pending.iter().position(|p| *p == bare) {
            Some(i) => {
                pending.swap_remove(i);
            }
            None => kept.push(line),
        }
    }
    match platform.write_atomic(&excluded.file, &kept.concat()) {
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
    worktree: &Path,
) -> (Vec<String>, Option<Excluded>) {
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
            Ok(p) if !p.contains('\n') => entries.push(anchored(p)),
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
    match exclude_lines(platform, worktree, &entries) {
        Ok((_, added)) if added.is_empty() => {}
        Ok((file, added)) => {
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

/// A path as an `info/exclude` line: anchored at the root, with glob
/// characters, a leading `#` or `!`, and a trailing space escaped.
fn anchored(path: &str) -> String {
    let mut line = String::from("/");
    for (i, c) in path.chars().enumerate() {
        if matches!(c, '*' | '?' | '[' | '\\') || (i == 0 && matches!(c, '#' | '!')) {
            line.push('\\');
        }
        line.push(c);
    }
    if line.ends_with(' ') {
        line.insert(line.len() - 1, '\\');
    }
    line
}
