//! The diff rules (VER-3): what a member's branch changed since its start,
//! and which of those changes are flags.

use crate::git::Git;
use crate::owns::Owns;
use crate::rules::Ruleset;
use crate::{TASK_FILE, VerifyError};

/// A flag before it is checked against the waivers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawFlag {
    pub(crate) id: String,
    pub(crate) detail: String,
}

impl RawFlag {
    pub(crate) fn new(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            detail: detail.into(),
        }
    }
}

/// The flags raised by the diff from `base` to `HEAD`.
pub(crate) fn inspect_diff(
    git: &Git<'_>,
    base: &str,
    owns: &[String],
    rules: &Ruleset,
) -> Result<Vec<RawFlag>, VerifyError> {
    let owns = Owns::new(owns);
    let mut flags = Vec::new();
    for pattern in &owns.invalid {
        flags.push(RawFlag::new(
            format!("bad-owns:{pattern}"),
            format!("the owns pattern {pattern:?} is not a valid glob"),
        ));
    }
    for (status, path) in git.changed_paths(base)? {
        if path == TASK_FILE {
            continue;
        }
        if status == 'D' && rules.is_test_file(&path) {
            flags.push(RawFlag::new(
                format!("test-deleted:{path}"),
                format!("test file deleted: {path}"),
            ));
        }
        if rules.is_check_config(&path) && !owns.matches(&path) {
            flags.push(RawFlag::new(
                format!("config-changed:{path}"),
                format!("check config changed outside owns: {path}"),
            ));
        }
        if !owns.is_empty() && !owns.matches(&path) {
            flags.push(RawFlag::new(
                format!("outside-owns:{path}"),
                format!("changed outside owns: {path}"),
            ));
        }
    }
    flags.extend(inspect_patch(git, base, rules)?);
    Ok(flags)
}

/// What one file of the patch has shown so far.
#[derive(Default)]
struct FileState {
    old_path: Option<String>,
    new_path: Option<String>,
    old_line: u32,
    new_line: u32,
    tests_added: usize,
    tests_removed: usize,
    /// Added lines, and removed lines, that fall in a lint table of a manifest.
    lint_added: Vec<u32>,
    lint_removed: Vec<u32>,
}

/// Reads the zero-context patch line by line: added lines that weaken a check,
/// a net loss of test markers, and edits to a manifest's lint tables.
fn inspect_patch(git: &Git<'_>, base: &str, rules: &Ruleset) -> Result<Vec<RawFlag>, VerifyError> {
    let patch = git.patch(base)?;
    let mut flags = Vec::new();
    let mut file = FileState::default();
    let mut in_header = false;
    let mut files: Vec<FileState> = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            files.push(std::mem::take(&mut file));
            in_header = true;
        } else if in_header && line.starts_with("--- ") {
            file.old_path = header_path(line, "a/");
        } else if in_header && line.starts_with("+++ ") {
            file.new_path = header_path(line, "b/");
        } else if line.starts_with("@@ ") {
            in_header = false;
            let (old, new) = hunk_start(line);
            file.old_line = old;
            file.new_line = new;
        } else if in_header {
            // `index`, `new file mode`, `Binary files ... differ`.
        } else if let Some(added) = line.strip_prefix('+') {
            if let Some(path) = file.new_path.clone()
                && path != TASK_FILE
            {
                if let Some(rule) = rules.weakening(&path, added) {
                    flags.push(RawFlag::new(
                        format!("weakened:{path}:{}", file.new_line),
                        format!(
                            "check weakened ({rule}) in {path}:{}: {}",
                            file.new_line,
                            clip(added)
                        ),
                    ));
                }
                if rules.is_test_marker(&path, added) {
                    file.tests_added += 1;
                }
                if rules.lint_sections(&path).is_some() {
                    file.lint_added.push(file.new_line);
                }
            }
            file.new_line += 1;
        } else if let Some(removed) = line.strip_prefix('-') {
            if let Some(path) = file.old_path.clone() {
                if file.new_path.is_some() && rules.is_test_marker(&path, removed) {
                    file.tests_removed += 1;
                }
                if rules.lint_sections(&path).is_some() {
                    file.lint_removed.push(file.old_line);
                }
            }
            file.old_line += 1;
        }
    }
    files.push(file);
    for file in files {
        flags.extend(file_flags(git, base, rules, &file));
    }
    Ok(flags)
}

fn file_flags(git: &Git<'_>, base: &str, rules: &Ruleset, file: &FileState) -> Vec<RawFlag> {
    let mut flags = Vec::new();
    let Some(path) = file.new_path.as_deref().or(file.old_path.as_deref()) else {
        return flags;
    };
    if file.new_path.is_some() && file.tests_removed > file.tests_added {
        flags.push(RawFlag::new(
            format!("test-removed:{path}"),
            format!(
                "{} test marker(s) removed and {} added in {path}",
                file.tests_removed, file.tests_added
            ),
        ));
    }
    let Some(sections) = rules.lint_sections(path) else {
        return flags;
    };
    if let Some(new_path) = &file.new_path
        && let Some(text) = git.file_at("HEAD", new_path)
    {
        let by_line = section_by_line(&text);
        for &n in &file.lint_added {
            if in_lint_section(&by_line, n, sections)
                && !text_line(&text, n).is_some_and(is_workspace_true)
            {
                flags.push(RawFlag::new(
                    format!("weakened:{new_path}:{n}"),
                    format!("lint table changed in {new_path}:{n}"),
                ));
            }
        }
    }
    if let Some(old_path) = &file.old_path
        && let Some(text) = git.file_at(base, old_path)
    {
        let by_line = section_by_line(&text);
        for &n in &file.lint_removed {
            if in_lint_section(&by_line, n, sections) {
                flags.push(RawFlag::new(
                    format!("lint-removed:{old_path}:{n}"),
                    format!("lint table line removed from {old_path}:{n}"),
                ));
            }
        }
    }
    flags
}

/// `[lints] workspace = true` opts a new crate into the workspace's own
/// table. It loosens nothing.
fn is_workspace_true(line: &str) -> bool {
    line.split_whitespace().collect::<String>() == "workspace=true"
}

fn text_line(text: &str, number: u32) -> Option<&str> {
    text.lines()
        .nth(usize::try_from(number).ok()?.checked_sub(1)?)
}

fn in_lint_section(by_line: &[String], line: u32, sections: &[String]) -> bool {
    let index = usize::try_from(line).ok().and_then(|n| n.checked_sub(1));
    index.and_then(|i| by_line.get(i)).is_some_and(|name| {
        sections.iter().any(|s| {
            name == s
                || name
                    .strip_prefix(s.as_str())
                    .is_some_and(|r| r.starts_with('.'))
        })
    })
}

/// Maps each line number of a TOML text to the table it sits in.
fn section_by_line(text: &str) -> Vec<String> {
    let mut map = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            current = trimmed
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .replace(['"', '\'', ' '], "");
        }
        map.push(current.clone());
    }
    map
}

/// `+++ b/path` or `--- a/path`; `None` for `/dev/null`.
fn header_path(line: &str, prefix: &str) -> Option<String> {
    let path = line.get(4..)?.trim_end_matches('\t');
    if path == "/dev/null" {
        return None;
    }
    Some(path.strip_prefix(prefix).unwrap_or(path).to_owned())
}

/// The first old and new line numbers of a `@@ -a,b +c,d @@` header.
fn hunk_start(line: &str) -> (u32, u32) {
    let number = |sign: char| -> u32 {
        line.split_whitespace()
            .find(|part| part.starts_with(sign))
            .and_then(|part| part.get(1..))
            .and_then(|part| part.split(',').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    (number('-'), number('+'))
}

fn clip(line: &str) -> String {
    line.trim().chars().take(120).collect()
}

#[cfg(test)]
mod tests;
