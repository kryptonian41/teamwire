//! The `spec-kit committed` line: the spec-kit files on disk that the commit
//! lacks. This line compares the working tree with the commit by design: the
//! lead edits the disk, and members get the commit.

use std::collections::{BTreeMap, BTreeSet};

use super::{Base, FEATURE_POINTER, command_folders};
use crate::cmd::doctor::Check;

/// At most this many paths are named, so a whole ignored folder stays one line.
const NAMED: usize = 8;

/// Why a file on disk is not what members get.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Changed,
    /// In git's index, but not in the commit.
    New,
    Untracked,
    /// The rule's file, line and pattern.
    Ignored(String, String, String),
}

impl State {
    fn text(&self) -> String {
        match self {
            Self::Changed => "changed".to_owned(),
            Self::New => "new".to_owned(),
            Self::Untracked => "untracked".to_owned(),
            Self::Ignored(source, line, pattern) => {
                format!("git-ignored by `{pattern}` in {source}:{line}")
            }
        }
    }
}

/// The name a path is shown under: a command folder's spec-kit entries as
/// one glob, any other path as itself. `None` for a path the line skips.
fn group(path: &str) -> Option<String> {
    if path == FEATURE_POINTER {
        return None;
    }
    if path.starts_with(".specify/") {
        return Some(path.to_owned());
    }
    command_folders().find_map(|(dir, start, end)| {
        let entry = path.strip_prefix(dir)?.strip_prefix('/')?;
        let entry = entry.split('/').next().unwrap_or(entry);
        (entry.starts_with(start) && entry.ends_with(end)).then(|| format!("{dir}/{start}*{end}"))
    })
}

fn nul_split(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The paths on disk that differ from the commit, each with its state.
fn differing(base: &Base<'_>) -> Result<Vec<(String, State)>, String> {
    let mut paths = vec![".specify"];
    paths.extend(command_folders().map(|(dir, _, _)| dir));
    let ask = |args: &[&str]| -> Result<Vec<String>, String> {
        let mut all = args.to_vec();
        all.push("--");
        all.extend(&paths);
        let out = base.git(&all)?;
        if out.exit_code != 0 {
            return Err(format!(
                "git {} failed ({})",
                args.first().copied().unwrap_or("git"),
                super::git_said(&out)
            ));
        }
        Ok(nul_split(&out.stdout))
    };
    let mut found = Vec::new();
    let diff = ask(&["diff", "--name-status", "--no-renames", "-z", base.sha()])?;
    for pair in diff.chunks(2) {
        if let [status, path] = pair {
            let state = match status.as_str() {
                "A" => State::New,
                "M" | "T" => State::Changed,
                // Deleted on disk: members still get the committed copy.
                _ => continue,
            };
            found.push((path.clone(), state));
        }
    }
    for path in ask(&["ls-files", "-z", "--others", "--exclude-standard"])? {
        found.push((path, State::Untracked));
    }
    let ignored: Vec<String> = ask(&[
        "ls-files",
        "-z",
        "--others",
        "--ignored",
        "--exclude-standard",
    ])?
    .into_iter()
    .filter(|p| group(p).is_some())
    .collect();
    if !ignored.is_empty() {
        let stdin: Vec<u8> = ignored
            .iter()
            .flat_map(|p| [p.as_bytes(), b"\0"])
            .flatten()
            .copied()
            .collect();
        let out = base.git_stdin(&["check-ignore", "-v", "-z", "--stdin"], stdin)?;
        if out.exit_code > 1 {
            return Err(format!(
                "git check-ignore failed ({})",
                super::git_said(&out)
            ));
        }
        for rule in nul_split(&out.stdout).chunks(4) {
            if let [source, line, pattern, path] = rule {
                // spec-kit's own ignore file keeps machine-local state out of commits.
                if source != ".specify/.gitignore" {
                    let state = State::Ignored(source.clone(), line.clone(), pattern.clone());
                    found.push((path.clone(), state));
                }
            }
        }
    }
    Ok(found)
}

/// A worktree is cut from a commit, so a spec-kit file that is only on disk,
/// only staged or git-ignored never reaches a worktree member.
pub(super) fn committed_check(base: &Base<'_>) -> Check {
    let name = "spec-kit committed";
    let at = base.at();
    let found = match differing(base) {
        Ok(found) => found,
        Err(e) => {
            return Check::warn(
                name,
                format!(
                    "could not compare the spec-kit files on disk with {at}: {e}; {}",
                    super::git_status_hint(&base.root)
                ),
            );
        }
    };
    let mut groups: BTreeMap<(String, State), usize> = BTreeMap::new();
    for (path, state) in found {
        if let Some(label) = group(&path) {
            *groups.entry((label, state)).or_default() += 1;
        }
    }
    if groups.is_empty() {
        return Check::pass(name, format!("every spec-kit file on disk is in {at}"));
    }
    let named: Vec<String> = groups
        .iter()
        .take(NAMED)
        .map(|((label, state), count)| match count {
            1 => format!("{label} ({})", state.text()),
            n => format!("{label} ({}, {n} files)", state.text()),
        })
        .collect();
    let more = match groups.len().saturating_sub(NAMED) {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    let rules: BTreeSet<String> = groups
        .keys()
        .filter_map(|(_, state)| match state {
            State::Ignored(source, line, pattern) => Some(format!(
                "remove the rule `{pattern}` from {source} (line {line})"
            )),
            State::Changed | State::New | State::Untracked => None,
        })
        .collect();
    let adds: BTreeSet<&str> = groups
        .keys()
        .map(|(label, _)| {
            if label.starts_with(".specify/") {
                ".specify"
            } else {
                label.as_str()
            }
        })
        .collect();
    let mut action: Vec<String> = rules.into_iter().collect();
    action.push(format!(
        "run `git add {}`",
        adds.into_iter().collect::<Vec<_>>().join(" ")
    ));
    let then = if base.checked_out {
        format!("then commit on {}", base.name)
    } else {
        // A commit here lands on the checked-out branch, not on the ref members start from.
        format!(
            "then, since {0} is not the checked-out branch, run `git switch {0}` and commit \
             there, or commit here and merge this branch into {0}",
            base.name
        )
    };
    Check::warn(
        name,
        format!(
            "on disk but not in {at}, so a worktree member does not get them: {}{more}; {}, {then}",
            named.join(", "),
            action.join(", "),
        ),
    )
}
