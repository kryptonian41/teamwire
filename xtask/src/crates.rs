//! The per-crate and top-level document rules of `docs/conventions.md`
//! section 1: crate naming, version and publish, the `//!` note, the
//! Components table, and the size of `AGENTS.md`.
//!
//! Never: change a file. Invariants: each broken rule adds one line naming
//! the file and the rule.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::{read_toml, relative};

const NOTE_MAX_LINES: usize = 15;
const AGENTS_MAX_LINES: usize = 120;

/// Naming, version, publish and the `//!` note for every member.
pub(crate) fn check_crates(
    root: &Path,
    members: &[PathBuf],
    problems: &mut Vec<String>,
) -> Result<()> {
    let workspace_package = read_toml(&root.join("Cargo.toml"))?
        .get("workspace")
        .and_then(|w| w.get("package"))
        .cloned();
    for dir in members {
        let manifest_path = dir.join("Cargo.toml");
        let name = relative(root, &manifest_path);
        let manifest = read_toml(&manifest_path)?;
        let package = manifest.get("package");
        let crate_name = package
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir.parent() == Some(&root.join("crates"))
            && (crate_name != folder || !crate_name.starts_with("teamwire-"))
        {
            problems.push(format!(
                "{name}: a crate under crates/ is named `teamwire-<x>` in a folder of the same name, found `{crate_name}` in `{folder}`"
            ));
        }
        if resolved(package, workspace_package.as_ref(), "version").and_then(toml::Value::as_str)
            != Some("0.0.0")
        {
            problems.push(format!("{name}: `version` must be \"0.0.0\""));
        }
        if resolved(package, workspace_package.as_ref(), "publish").and_then(toml::Value::as_bool)
            != Some(false)
        {
            problems.push(format!("{name}: `publish` must be false"));
        }
        for entry in ["src/lib.rs", "src/main.rs"] {
            let path = dir.join(entry);
            if path.is_file() {
                check_note(root, &path, problems);
            }
        }
    }
    Ok(())
}

/// A package key set directly, or inherited with `key.workspace = true`.
fn resolved<'a>(
    package: Option<&'a toml::Value>,
    workspace_package: Option<&'a toml::Value>,
    key: &str,
) -> Option<&'a toml::Value> {
    let value = package?.get(key)?;
    let inherits = value.get("workspace").and_then(toml::Value::as_bool) == Some(true);
    if inherits {
        workspace_package?.get(key)
    } else {
        Some(value)
    }
}

/// The file opens with a `//!` comment of 1 to 15 lines.
fn check_note(root: &Path, path: &Path, problems: &mut Vec<String>) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines = text
        .lines()
        .take_while(|l| l.trim_start().starts_with("//!"))
        .count();
    let name = relative(root, path);
    if lines == 0 {
        problems.push(format!(
            "{name}: must open with a `//!` note (purpose, what it must never do, invariants)"
        ));
    } else if lines > NOTE_MAX_LINES {
        problems.push(format!(
            "{name}: the `//!` note is {lines} lines; the limit is {NOTE_MAX_LINES}"
        ));
    }
}

/// One Components row per crate under `crates/`, and no row without a crate.
pub(crate) fn check_components(root: &Path, problems: &mut Vec<String>) {
    let text = std::fs::read_to_string(root.join("docs/ARCHITECTURE.md")).unwrap_or_default();
    let mut in_section = false;
    let mut rows: Vec<String> = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            in_section = heading.trim() == "Components";
        } else if in_section && line.starts_with('|') {
            let cell = line.trim_start_matches('|').split('|').next().unwrap_or("");
            if let Some(name) = cell
                .split('`')
                .nth(1)
                .filter(|n| n.starts_with("teamwire-"))
            {
                rows.push(name.to_owned());
            }
        }
    }
    let mut folders: Vec<String> = std::fs::read_dir(root.join("crates"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().join("Cargo.toml").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    folders.sort();
    for folder in &folders {
        if !rows.contains(folder) {
            problems.push(format!(
                "docs/ARCHITECTURE.md: the Components table has no row for crates/{folder}"
            ));
        }
    }
    for row in rows {
        if !folders.contains(&row) {
            problems.push(format!(
                "docs/ARCHITECTURE.md: the Components table has a row for `{row}` but crates/{row} does not exist"
            ));
        }
    }
}

/// `AGENTS.md` stays under 120 lines.
pub(crate) fn check_agents_md(root: &Path, problems: &mut Vec<String>) {
    let lines = std::fs::read_to_string(root.join("AGENTS.md"))
        .map(|t| t.lines().count())
        .unwrap_or(0);
    if lines == 0 {
        problems.push("AGENTS.md: missing or empty".to_owned());
    } else if lines >= AGENTS_MAX_LINES {
        problems.push(format!(
            "AGENTS.md: {lines} lines; it must stay under {AGENTS_MAX_LINES}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::fixture::{NOTE, clean_workspace, empty_metadata, write};
    use crate::tidy;

    fn problems(root: &std::path::Path) -> Vec<String> {
        tidy(root, &empty_metadata()).unwrap().problems
    }

    fn only(problems: &[String], needle: &str) {
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems.first().is_some_and(|p| p.contains(needle)),
            "{problems:?}"
        );
    }

    #[test]
    fn crate_named_differently_from_its_folder_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/Cargo.toml",
            "[package]\nname = \"teamwire-b\"\nversion.workspace = true\npublish.workspace = true\n\n[lints]\nworkspace = true\n",
        );
        only(&problems(dir.path()), "a folder of the same name");
    }

    #[test]
    fn crate_without_the_teamwire_prefix_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/plain/Cargo.toml",
            "[package]\nname = \"plain\"\nversion.workspace = true\npublish.workspace = true\n\n[lints]\nworkspace = true\n",
        );
        write(dir.path(), "crates/plain/src/lib.rs", NOTE);
        let found = problems(dir.path());
        assert!(
            found.iter().any(|p| p.contains("`teamwire-<x>`")),
            "{found:?}"
        );
    }

    #[test]
    fn version_other_than_zero_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/Cargo.toml",
            "[package]\nname = \"teamwire-a\"\nversion = \"1.0.0\"\npublish.workspace = true\n\n[lints]\nworkspace = true\n",
        );
        only(&problems(dir.path()), "`version` must be");
    }

    #[test]
    fn publishable_crate_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/Cargo.toml",
            "[package]\nname = \"teamwire-a\"\nversion.workspace = true\n\n[lints]\nworkspace = true\n",
        );
        only(&problems(dir.path()), "`publish` must be false");
    }

    #[test]
    fn missing_note_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/src/lib.rs",
            "pub fn f() {}\n",
        );
        only(&problems(dir.path()), "must open with a `//!` note");
    }

    #[test]
    fn note_of_sixteen_lines_fails_and_fifteen_passes() {
        let dir = clean_workspace();
        write(dir.path(), "xtask/src/main.rs", &"//! line\n".repeat(15));
        assert_eq!(problems(dir.path()), Vec::<String>::new());
        write(dir.path(), "xtask/src/main.rs", &"//! line\n".repeat(16));
        only(&problems(dir.path()), "the limit is 15");
    }

    #[test]
    fn crate_without_a_components_row_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-b/Cargo.toml",
            "[package]\nname = \"teamwire-b\"\nversion.workspace = true\npublish.workspace = true\n\n[lints]\nworkspace = true\n",
        );
        write(dir.path(), "crates/teamwire-b/src/lib.rs", NOTE);
        only(&problems(dir.path()), "no row for crates/teamwire-b");
    }

    #[test]
    fn components_row_without_a_crate_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "docs/ARCHITECTURE.md",
            "## Components\n\n| Crate | x |\n| --- | --- |\n| `teamwire-a` | a |\n| `teamwire-gone` | b |\n",
        );
        only(&problems(dir.path()), "crates/teamwire-gone does not exist");
    }

    #[test]
    fn agents_md_of_120_lines_fails_and_119_passes() {
        let dir = clean_workspace();
        write(dir.path(), "AGENTS.md", &"line\n".repeat(119));
        assert_eq!(problems(dir.path()), Vec::<String>::new());
        write(dir.path(), "AGENTS.md", &"line\n".repeat(120));
        only(&problems(dir.path()), "AGENTS.md: 120 lines");
    }
}
