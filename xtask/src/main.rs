//! `cargo xtask tidy`: the structure rules of `docs/conventions.md` section 1
//! and the layers of section 5, plus, with `--base <ref>`, a report of
//! protected files changed and tests weakened since the merge-base.
//!
//! Never: change a file; pass when a rule is broken.
//!
//! Invariants: each broken rule prints one line naming the file and the
//! rule, and the command fails when any such line was printed. Report lines
//! (`PROTECTED-CHANGED`, `WEAKENED`, a file over 400 lines) are printed but
//! never fail the command: the lead reads them at each integration.

mod adapters;
mod base;
mod code_text;
mod crates;
#[cfg(test)]
mod fixture;
mod layers;
mod sizes;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use serde_json::Value;

const USAGE: &str = "usage: cargo xtask tidy [--base <ref>]";

/// What one run found. `problems` fail the command; `notes` are reports.
#[derive(Debug, Default)]
struct Findings {
    problems: Vec<String>,
    notes: Vec<String>,
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(found) => {
            for note in &found.notes {
                say(note);
            }
            if found.problems.is_empty() {
                say("tidy: ok");
                return ExitCode::SUCCESS;
            }
            for problem in &found.problems {
                say(problem);
            }
            say(&format!("tidy: {} problem(s)", found.problems.len()));
            ExitCode::FAILURE
        }
        Err(e) => {
            say(&format!("tidy: {e:#}"));
            ExitCode::FAILURE
        }
    }
}

#[expect(clippy::print_stdout, reason = "xtask reports to the terminal")]
fn say(line: &str) {
    println!("{line}");
}

/// The `--base` ref of `tidy [--base <ref>]`.
fn parse_args(args: Vec<String>) -> Result<Option<String>> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("tidy") => {}
        _ => bail!(USAGE),
    }
    let mut base = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base" => base = Some(args.next().context(USAGE)?),
            _ => bail!("unknown argument {arg:?}; {USAGE}"),
        }
    }
    Ok(base)
}

fn run(args: Vec<String>) -> Result<Findings> {
    let base = parse_args(args)?;
    let root = repo_root()?;
    let metadata = layers::cargo_metadata(&root)?;
    let mut found = tidy(&root, &metadata)?;
    if let Some(base) = base {
        base::check_base(&root, &base, &mut found)?;
    }
    Ok(found)
}

/// The workspace the command runs in, found from the current directory. A
/// compile-time path would make a binary built in another checkout (a
/// worktree's `target/` is cloned from the main one) check the wrong tree.
fn repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("reading the current directory")?;
    workspace_root(&cwd)
}

/// The nearest directory at or above `start` whose `Cargo.toml` has a
/// `[workspace]` table.
fn workspace_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() && read_toml(&manifest)?.contains_key("workspace") {
            return Ok(dir.to_path_buf());
        }
    }
    bail!("no workspace Cargo.toml at or above {}", start.display())
}

/// Every rule but `--base`, over the repository at `root`. `metadata` is the
/// output of `cargo metadata --no-deps`, which the layer rule reads.
fn tidy(root: &Path, metadata: &Value) -> Result<Findings> {
    let mut found = Findings::default();
    let members = members(root)?;
    for member in &members {
        check_manifest(root, member, &mut found.problems)?;
    }
    crates::check_crates(root, &members, &mut found.problems)?;
    crates::check_components(root, &mut found.problems);
    crates::check_agents_md(root, &mut found.problems);
    check_no_mod_rs(root, root, &mut found.problems)?;
    check_claude_md(root, &mut found.problems);
    sizes::check_sizes(root, &mut found)?;
    layers::check_layers(metadata, &mut found.problems);
    adapters::check_adapters(root, &mut found.problems)?;
    Ok(found)
}

/// The directories of every workspace member, from the root manifest's
/// `members` list. A `dir/*` entry means every subdirectory with a `Cargo.toml`.
fn members(root: &Path) -> Result<Vec<PathBuf>> {
    let manifest = read_toml(&root.join("Cargo.toml"))?;
    let list = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml::Value::as_array)
        .context("Cargo.toml has no [workspace] members list")?;
    let mut dirs = Vec::new();
    for entry in list {
        let entry = entry
            .as_str()
            .context("a workspace member is not a string")?;
        if let Some(parent) = entry.strip_suffix("/*") {
            let mut found: Vec<PathBuf> = std::fs::read_dir(root.join(parent))
                .with_context(|| format!("reading {parent}"))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.join("Cargo.toml").is_file())
                .collect();
            found.sort();
            dirs.extend(found);
        } else {
            dirs.push(root.join(entry));
        }
    }
    Ok(dirs)
}

pub(crate) fn read_toml(path: &Path) -> Result<toml::Table> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.parse::<toml::Table>()
        .with_context(|| format!("parsing {}", path.display()))
}

pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// `[lints] workspace = true`, and every dependency `workspace = true`.
fn check_manifest(root: &Path, dir: &Path, problems: &mut Vec<String>) -> Result<()> {
    let path = dir.join("Cargo.toml");
    let name = relative(root, &path);
    let manifest = read_toml(&path)?;
    let lints_opt_in = manifest
        .get("lints")
        .and_then(|l| l.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true);
    if !lints_opt_in {
        problems.push(format!("{name}: missing `[lints] workspace = true`"));
    }
    let mut tables: Vec<(String, &toml::Value)> = Vec::new();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = manifest.get(kind) {
            tables.push((kind.to_owned(), t));
        }
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            for (target, spec) in targets {
                if let Some(t) = spec.get(kind) {
                    tables.push((format!("target.{target}.{kind}"), t));
                }
            }
        }
    }
    for (kind, table) in tables {
        let Some(table) = table.as_table() else {
            continue;
        };
        for (dep, spec) in table {
            let from_workspace = spec.get("workspace").and_then(toml::Value::as_bool) == Some(true);
            if !from_workspace {
                problems.push(format!("{name}: [{kind}] {dep} is not `workspace = true`"));
            }
        }
    }
    Ok(())
}

/// No `mod.rs` anywhere outside build output and git data.
fn check_no_mod_rs(root: &Path, dir: &Path, problems: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if matches!(file_name, "target" | ".git" | ".worktrees") {
                continue;
            }
            check_no_mod_rs(root, &path, problems)?;
        } else if file_name == "mod.rs" {
            problems.push(format!(
                "{}: mod.rs is not allowed; name the file after its module",
                relative(root, &path)
            ));
        }
    }
    Ok(())
}

fn check_claude_md(root: &Path, problems: &mut Vec<String>) {
    let text = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap_or_default();
    if text.lines().next() != Some("@AGENTS.md") {
        problems.push("CLAUDE.md: the first line must be `@AGENTS.md`".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{clean_workspace, empty_metadata, write};

    fn tidy_problems(root: &Path) -> Vec<String> {
        tidy(root, &empty_metadata()).unwrap().problems
    }

    #[test]
    fn clean_workspace_passes() {
        let dir = clean_workspace();
        assert_eq!(tidy_problems(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn missing_lints_opt_in_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/Cargo.toml",
            "[package]\nname = \"teamwire-a\"\nversion.workspace = true\npublish.workspace = true\n",
        );
        let problems = tidy_problems(dir.path());
        assert_eq!(
            problems,
            ["crates/teamwire-a/Cargo.toml: missing `[lints] workspace = true`"]
        );
    }

    #[test]
    fn dependency_outside_workspace_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "xtask/Cargo.toml",
            "[package]\nname = \"xtask\"\nversion.workspace = true\npublish.workspace = true\n\n[dev-dependencies]\nserde = \"1\"\n\n[lints]\nworkspace = true\n",
        );
        let problems = tidy_problems(dir.path());
        assert_eq!(
            problems,
            ["xtask/Cargo.toml: [dev-dependencies] serde is not `workspace = true`"]
        );
    }

    #[test]
    fn mod_rs_fails() {
        let dir = clean_workspace();
        write(dir.path(), "crates/teamwire-a/src/x/mod.rs", "");
        let problems = tidy_problems(dir.path());
        assert_eq!(problems.len(), 1);
        assert!(
            problems
                .first()
                .is_some_and(|p| p.starts_with("crates/teamwire-a/src/x/mod.rs:")),
            "{problems:?}"
        );
    }

    #[test]
    fn claude_md_without_import_fails() {
        let dir = clean_workspace();
        write(dir.path(), "CLAUDE.md", "# teamwire\n");
        let problems = tidy_problems(dir.path());
        assert_eq!(problems, ["CLAUDE.md: the first line must be `@AGENTS.md`"]);
    }

    #[test]
    fn workspace_root_is_found_from_a_nested_directory_of_the_tree_being_checked() {
        let dir = clean_workspace();
        let nested = dir.path().join("crates/teamwire-a/src");
        assert_eq!(workspace_root(&nested).unwrap(), dir.path());
        assert_eq!(workspace_root(dir.path()).unwrap(), dir.path());
    }

    #[test]
    fn workspace_root_skips_a_member_manifest_and_fails_outside_a_workspace() {
        let dir = clean_workspace();
        // crates/teamwire-a/Cargo.toml has no [workspace] table.
        let member = dir.path().join("crates/teamwire-a");
        assert_eq!(workspace_root(&member).unwrap(), dir.path());
        let outside = tempfile::tempdir().unwrap();
        assert!(workspace_root(outside.path()).is_err());
    }

    #[test]
    fn base_is_accepted_and_unknown_flags_refused() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            parse_args(args(&["tidy", "--base", "main"])).unwrap(),
            Some("main".to_owned())
        );
        assert_eq!(parse_args(args(&["tidy"])).unwrap(), None);
        assert!(parse_args(args(&["tidy", "--base"])).is_err());
        assert!(parse_args(args(&["tidy", "--nope"])).is_err());
        assert!(parse_args(args(&["lint"])).is_err());
    }
}
