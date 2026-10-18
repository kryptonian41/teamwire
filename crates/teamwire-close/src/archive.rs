//! The archive of a closed team: `team.db` from `SQLite`'s backup API, every
//! other file of the team directory, and `SHA256SUMS`. It is checked by
//! content before anything is deleted, and a path is never overwritten (CLS-2).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use teamwire_core::{TeamState, Timestamp};
use teamwire_store::{Ctx, Store};

use crate::CloseOpts;

/// `<parent>/<team>-<YYYY-MM-DD>`, then `-HHMMSS`, then `-HHMMSS-<n>` when the
/// earlier names are taken. The parent is `--archive-to`, else
/// `$TEAMWIRE_HOME/archive`. Nothing is created here.
pub(crate) fn destination(
    ctx: &Ctx<'_>,
    team: &teamwire_core::TeamName,
    opts: &CloseOpts,
) -> PathBuf {
    let parent = opts
        .archive_to
        .clone()
        .unwrap_or_else(|| ctx.home.join("archive"));
    first_free(&parent, team.as_str(), ctx.clock.now())
}

fn first_free(parent: &Path, team: &str, now: Timestamp) -> PathBuf {
    let at = now.as_datetime();
    let day = format!(
        "{team}-{:04}-{:02}-{:02}",
        at.year(),
        u8::from(at.month()),
        at.day()
    );
    let stamped = format!("{day}-{:02}{:02}{:02}", at.hour(), at.minute(), at.second());
    let mut candidates = vec![day, stamped.clone()];
    candidates.extend((2..1000).map(|n| format!("{stamped}-{n}")));
    let taken = |name: &String| parent.join(name).symlink_metadata().is_ok();
    let name = candidates
        .iter()
        .find(|n| !taken(n))
        .cloned()
        .unwrap_or(stamped);
    parent.join(name)
}

/// `team.db` and the files `SQLite` keeps beside it. The backup API writes a
/// whole, current copy of the database, so the live files are never copied.
const DB_FILES: [&str; 4] = ["team.db", "team.db-wal", "team.db-shm", "team.db-journal"];
const SUMS: &str = "SHA256SUMS";
/// The next `SHA256SUMS`, written before `team.db` is replaced and renamed
/// into place after, so a file that checks out exists at every instant.
const SUMS_NEXT: &str = "SHA256SUMS.next";
/// The second backup, while it is being made.
const DB_NEW: &str = "team.db.new";

/// One file of the archive: its path under the archive and its content hash.
type Sums = BTreeMap<String, String>;

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Paths under a directory, relative to it, each list sorted.
struct Tree {
    dirs: Vec<PathBuf>,
    files: Vec<PathBuf>,
    links: Vec<PathBuf>,
}

/// What the team directory holds, other than the database: every directory
/// and every regular file, as paths under the team directory. A symbolic link
/// is left out and named in the second list, because a copy of its target
/// could reach outside the team's own files.
fn walk(team_dir: &Path) -> Result<Tree, String> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut links = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(rel) = pending.pop() {
        let here = team_dir.join(&rel);
        let entries =
            std::fs::read_dir(&here).map_err(|e| format!("cannot read {}: {e}", here.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot read {}: {e}", here.display()))?;
            let child = rel.join(entry.file_name());
            if rel.as_os_str().is_empty() && DB_FILES.iter().any(|d| entry.file_name() == *d) {
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(|e| format!("cannot stat {}: {e}", child.display()))?;
            if kind.is_symlink() {
                links.push(child);
            } else if kind.is_dir() {
                dirs.push(child.clone());
                pending.push(child);
            } else if kind.is_file() {
                files.push(child);
            } else {
                links.push(child);
            }
        }
    }
    dirs.sort();
    files.sort();
    Ok(Tree { dirs, files, links })
}

fn slash(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn write_sums(ctx: &Ctx<'_>, dest: &Path, sums: &Sums, name: &str) -> Result<(), String> {
    let text = sums.iter().fold(String::new(), |mut text, (path, hash)| {
        // Writing to a `String` cannot fail.
        writeln!(text, "{hash}  {path}").ok();
        text
    });
    ctx.platform
        .write_atomic(&dest.join(name), text.as_bytes())
        .map_err(|e| format!("cannot write {name}: {e}"))
}

fn read_sums(dest: &Path) -> Result<Sums, String> {
    read_sums_named(dest, SUMS)
}

fn read_sums_named(dest: &Path, name: &str) -> Result<Sums, String> {
    let text = std::fs::read_to_string(dest.join(name))
        .map_err(|e| format!("cannot read {name} in {}: {e}", dest.display()))?;
    let mut sums = Sums::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (hash, path) = line
            .split_once("  ")
            .filter(|(hash, _)| hash.len() == 64)
            .ok_or_else(|| format!("{name} has a line that is not `<sha256>  <path>`: {line}"))?;
        sums.insert(path.to_owned(), hash.to_owned());
    }
    Ok(sums)
}

/// Where an archive is built before it gets its final name. A team name has
/// no hyphen, so `.partial-<team>-` never starts another team's folder. The
/// folder has no `SHA256SUMS` until its copy is whole, and `teamwire metrics`
/// skips an archive folder without one.
const PARTIAL: &str = ".partial-";

/// Writes the archive: `team.db` from the backup API, a copy of every other
/// file with its mode, and `SHA256SUMS`, all in a temporary folder that is
/// renamed to `dest` (a path that must not exist yet) once whole. A close
/// killed part way therefore never leaves a half archive under the final
/// name, and the next close removes this team's leftover temporary folders.
/// The text returned says how many files went in.
pub(crate) fn write(
    ctx: &Ctx<'_>,
    team: &teamwire_core::TeamName,
    team_dir: &Path,
    dest: &Path,
) -> Result<String, String> {
    let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) else {
        return Err(format!("{} has no folder name", dest.display()));
    };
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    remove_leftovers(parent, team)?;
    let tmp = parent.join(format!("{PARTIAL}{}", name.to_string_lossy()));
    // `create_dir`, not `create_dir_all`: it fails when the path exists.
    std::fs::create_dir(&tmp).map_err(|e| format!("cannot make {}: {e}", tmp.display()))?;
    let summary = fill(ctx, team_dir, &tmp, dest)?;
    // `rename` replaces an empty folder, so check first: an archive is never
    // written over another (CLS-2).
    if dest.symlink_metadata().is_ok() {
        return Err(format!("{} already exists", dest.display()));
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "renaming a finished folder is the one atomic step that names the archive; write_atomic writes a file"
    )]
    let moved = std::fs::rename(&tmp, dest);
    moved.map_err(|e| format!("cannot move {} to {}: {e}", tmp.display(), dest.display()))?;
    Ok(summary)
}

/// Removes the temporary folders an earlier close of this team left.
fn remove_leftovers(parent: &Path, team: &teamwire_core::TeamName) -> Result<(), String> {
    let prefix = format!("{PARTIAL}{team}-");
    let entries =
        std::fs::read_dir(parent).map_err(|e| format!("cannot read {}: {e}", parent.display()))?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let path = entry.path();
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("cannot remove the leftover {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// The body of `write`, into `dest`; `shown` is the path the summary names.
fn fill(ctx: &Ctx<'_>, team_dir: &Path, dest: &Path, shown: &Path) -> Result<String, String> {
    ctx.store
        .backup_to(&dest.join("team.db"))
        .map_err(|e| format!("the database backup failed: {e}"))?;
    let Tree { dirs, files, links } = walk(team_dir)?;
    for dir in &dirs {
        std::fs::create_dir_all(dest.join(dir))
            .map_err(|e| format!("cannot make {}: {e}", dest.join(dir).display()))?;
    }
    let mut sums = Sums::new();
    for rel in &files {
        let from = team_dir.join(rel);
        let bytes =
            std::fs::read(&from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
        let to = dest.join(rel);
        ctx.platform
            .write_atomic(&to, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", to.display()))?;
        copy_mode(&from, &to);
        sums.insert(slash(rel), sha256(&bytes));
    }
    let db = std::fs::read(dest.join("team.db"))
        .map_err(|e| format!("cannot read the archived database: {e}"))?;
    sums.insert("team.db".to_owned(), sha256(&db));
    write_sums(ctx, dest, &sums, SUMS)?;
    let skipped = if links.is_empty() {
        String::new()
    } else {
        format!(
            "; left out {} link(s): {}",
            links.len(),
            links
                .iter()
                .map(|l| slash(l))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Ok(format!(
        "{} file(s) and team.db in {}{skipped}",
        files.len(),
        shown.display()
    ))
}

/// A secret a member kept in a private file stays private in the archive.
fn copy_mode(from: &Path, to: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(meta) = std::fs::metadata(from) {
            let mode = meta.permissions().mode() & 0o777;
            if let Err(e) = std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode)) {
                tracing::warn!(path = %to.display(), error = %e, "archive file mode not copied");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (from, to);
    }
}

/// Checks the archive by content (CLS-2). Every file `SHA256SUMS` names must
/// hash to what it says, the archive must hold nothing else, every file in the
/// team directory must be in the archive with the same hash, and the
/// archived database must read, pass `integrity_check` and describe the same
/// team. `expect_closed` adds that it records the team as closed.
pub(crate) fn verify(
    ctx: &Ctx<'_>,
    team_dir: &Path,
    dest: &Path,
    expect_closed: bool,
) -> Result<(), String> {
    verify_files(team_dir, dest)?;
    verify_db(ctx, dest, expect_closed)
}

/// The file checks of `verify`. While the second backup replaces `team.db`,
/// the archive may hold a `SHA256SUMS.next`; `team.db` then may match either
/// list, and nothing else may differ between them.
fn verify_files(team_dir: &Path, dest: &Path) -> Result<(), String> {
    let sums = read_sums(dest)?;
    let next = if dest.join(SUMS_NEXT).is_file() {
        Some(read_sums_named(dest, SUMS_NEXT)?)
    } else {
        None
    };
    for (path, want) in &sums {
        let file = dest.join(path);
        let bytes = std::fs::read(&file)
            .map_err(|e| format!("archived file {path} cannot be read: {e}"))?;
        let got = sha256(&bytes);
        let also = next
            .as_ref()
            .filter(|_| path == "team.db")
            .and_then(|n| n.get(path));
        if &got != want && also != Some(&got) {
            return Err(format!(
                "archived file {path} does not match {SUMS}: the archive is not trusted"
            ));
        }
    }
    let archived = walk(dest)?;
    let extra: Vec<String> = archived
        .files
        .iter()
        .map(|p| slash(p))
        .filter(|p| ![SUMS, SUMS_NEXT, "team.db", DB_NEW].contains(&p.as_str()))
        .filter(|p| !sums.contains_key(p))
        .collect();
    if !extra.is_empty() {
        return Err(format!(
            "the archive holds files {SUMS} does not list: {}",
            extra.join(", ")
        ));
    }
    let sources = walk(team_dir)?;
    for rel in &sources.files {
        let name = slash(rel);
        let bytes = std::fs::read(team_dir.join(rel))
            .map_err(|e| format!("cannot read {}: {e}", team_dir.join(rel).display()))?;
        match sums.get(&name) {
            Some(hash) if *hash == sha256(&bytes) => {}
            Some(_) => {
                return Err(format!(
                    "{name} changed after it was archived; nothing was deleted"
                ));
            }
            None => {
                return Err(format!(
                    "{name} is new since the archive was taken; nothing was deleted"
                ));
            }
        }
    }
    Ok(())
}

fn verify_db(ctx: &Ctx<'_>, dest: &Path, expect_closed: bool) -> Result<(), String> {
    let db = Store::open_read_only_at(&dest.join("team.db"))
        .map_err(|e| format!("the archived database cannot be opened: {e}"))?;
    let integrity = db
        .integrity_check()
        .map_err(|e| format!("the archived database cannot be checked: {e}"))?;
    if integrity != "ok" {
        return Err(format!("the archived database is damaged: {integrity}"));
    }
    let archived = read_team(&db)?;
    let live = read_team(ctx.store)?;
    if archived != live {
        return Err("the archived database describes a different team state".to_owned());
    }
    if expect_closed && archived.state != TeamState::Closed {
        return Err("the archived database does not record the team as closed".to_owned());
    }
    Ok(())
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn read_team(store: &Store) -> Result<teamwire_store::TeamRow, String> {
    store
        .read(|tx| tx.team())
        .map_err(|e| format!("the team row cannot be read: {e}"))?
        .ok_or_else(|| "the database has no team row".to_owned())
}

/// Replaces the archive's `team.db` with a new backup, so the last event of
/// the close (`team.closed`) is in it, and rewrites `SHA256SUMS` to match.
/// The order keeps the archive checkable at every instant: the new sums go to
/// `SHA256SUMS.next` first, `verify` accepts `team.db` under either list, and
/// the sums are replaced last.
pub(crate) fn refresh_db(ctx: &Ctx<'_>, dest: &Path) -> Result<(), String> {
    refresh_db_observed(ctx, dest, &|_| {})
}

fn refresh_db_observed(ctx: &Ctx<'_>, dest: &Path, after: &dyn Fn(&str)) -> Result<(), String> {
    let fresh = dest.join(DB_NEW);
    // A crash in an earlier attempt may have left a partial backup.
    match std::fs::remove_file(&fresh) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot remove {}: {e}", fresh.display())),
    }
    ctx.store
        .backup_to(&fresh)
        .map_err(|e| format!("the second database backup failed: {e}"))?;
    let bytes =
        std::fs::read(&fresh).map_err(|e| format!("cannot read {}: {e}", fresh.display()))?;
    let mut sums = read_sums(dest)?;
    sums.insert("team.db".to_owned(), sha256(&bytes));
    write_sums(ctx, dest, &sums, SUMS_NEXT)?;
    after("sums_next");
    ctx.platform
        .write_atomic(&dest.join("team.db"), &bytes)
        .map_err(|e| format!("cannot replace the archived database: {e}"))?;
    after("db_replaced");
    write_sums(ctx, dest, &sums, SUMS)?;
    after("sums_replaced");
    for name in [SUMS_NEXT, DB_NEW] {
        std::fs::remove_file(dest.join(name)).map_err(|e| format!("cannot remove {name}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
