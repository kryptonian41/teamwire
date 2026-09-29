//! `cargo xtask tidy`: the structure rules of `docs/conventions.md` section 1
//! that a program checks. This minimal version checks lint opt-in, workspace
//! dependencies, `mod.rs` files and `CLAUDE.md`'s first line.
//!
//! Never: change a file; pass when a rule is broken.
//!
//! Invariants: each broken rule prints one line naming the file and the
//! rule, and the command fails when any line was printed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: cargo xtask tidy [--base <ref>]";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(problems) if problems.is_empty() => {
            say("tidy: ok");
            ExitCode::SUCCESS
        }
        Ok(problems) => {
            for problem in &problems {
                say(problem);
            }
            say(&format!("tidy: {} problem(s)", problems.len()));
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

fn run(args: Vec<String>) -> Result<Vec<String>> {
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
    if let Some(base) = base {
        say(&format!(
            "tidy: --base {base}: the protected-file and weakened-test checks are not built yet"
        ));
    }
    tidy(&repo_root()?)
}

fn repo_root() -> Result<PathBuf> {
    let xtask_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    xtask_dir
        .parent()
        .map(Path::to_path_buf)
        .context("xtask has no parent directory")
}

/// Every rule, over the repository at `root`. Returns one line per problem.
fn tidy(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for member in members(root)? {
        check_manifest(root, &member, &mut problems)?;
    }
    check_no_mod_rs(root, root, &mut problems)?;
    check_claude_md(root, &mut problems);
    Ok(problems)
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

fn read_toml(path: &Path) -> Result<toml::Table> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.parse::<toml::Table>()
        .with_context(|| format!("parsing {}", path.display()))
}

fn relative(root: &Path, path: &Path) -> String {
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

    fn write(root: &Path, path: &str, text: &str) {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        #[expect(
            clippy::disallowed_methods,
            reason = "test fixtures in a temporary directory"
        )]
        std::fs::write(full, text).unwrap();
    }

    fn clean_workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\", \"xtask\"]\n",
        );
        write(root, "CLAUDE.md", "@AGENTS.md\n");
        let good = "[package]\nname = \"a\"\n\n[dependencies]\nserde = { workspace = true }\n\n[lints]\nworkspace = true\n";
        write(root, "crates/teamwire-a/Cargo.toml", good);
        write(root, "crates/teamwire-a/src/lib.rs", "");
        write(root, "xtask/Cargo.toml", good);
        dir
    }

    #[test]
    fn clean_workspace_passes() {
        let dir = clean_workspace();
        assert_eq!(tidy(dir.path()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn missing_lints_opt_in_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/Cargo.toml",
            "[package]\nname = \"a\"\n",
        );
        let problems = tidy(dir.path()).unwrap();
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
            "[dev-dependencies]\nserde = \"1\"\n\n[lints]\nworkspace = true\n",
        );
        let problems = tidy(dir.path()).unwrap();
        assert_eq!(
            problems,
            ["xtask/Cargo.toml: [dev-dependencies] serde is not `workspace = true`"]
        );
    }

    #[test]
    fn mod_rs_fails() {
        let dir = clean_workspace();
        write(dir.path(), "crates/teamwire-a/src/x/mod.rs", "");
        let problems = tidy(dir.path()).unwrap();
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
        let problems = tidy(dir.path()).unwrap();
        assert_eq!(problems, ["CLAUDE.md: the first line must be `@AGENTS.md`"]);
    }

    #[test]
    fn base_is_accepted_and_unknown_flags_refused() {
        assert!(run(vec!["tidy".into(), "--base".into()]).is_err());
        assert!(run(vec!["tidy".into(), "--nope".into()]).is_err());
        assert!(run(vec!["lint".into()]).is_err());
    }
}
