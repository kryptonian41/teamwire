//! The store rule of `docs/conventions.md`: only `teamwire-store` calls
//! `.connection()`, because that call reaches `SQLite` without `Store::write`.
//!
//! Never: change a file. Invariants: each non-test `.connection()` outside
//! `crates/teamwire-store/src/` adds one line naming the file and the line.
//! Test code is skipped: files named `tests.rs`, files under a `tests`
//! directory, and everything from a `#[cfg(test)] mod` line on, and every file of a crate under `tests/`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::code_text::code_only;
use crate::relative;

const STORE_SRC: &str = "crates/teamwire-store/src/";

pub(crate) fn check_connection(
    root: &Path,
    members: &[std::path::PathBuf],
    problems: &mut Vec<String>,
) -> Result<()> {
    let mut files = Vec::new();
    for member in members {
        crate::adapters::rust_files(&member.join("src"), &mut files)?;
    }
    files.sort();
    for path in files {
        let name = relative(root, &path);
        let is_test_file =
            name.ends_with("/tests.rs") || name.contains("/tests/") || name.starts_with("tests/");
        if name.starts_with(STORE_SRC) || is_test_file {
            continue;
        }
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {name}"))?;
        let code = code_only(&text);
        let mut lines = code.lines().enumerate().peekable();
        while let Some((index, line)) = lines.next() {
            // A `#[cfg(test)]` on a module ends the production code; on a const or use it does not.
            if line.contains("#[cfg(test)]")
                && lines
                    .peek()
                    .is_some_and(|(_, next)| next.trim_start().starts_with("mod "))
            {
                break;
            }
            if line.contains(".connection()") {
                problems.push(format!(
                    "{name}:{}: calls .connection(); only teamwire-store reaches SQLite, through Store::write",
                    index + 1
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::fixture::{clean_workspace, empty_metadata, write};
    use crate::tidy;

    fn hits(dir: &std::path::Path) -> Vec<String> {
        let found = tidy(dir, &empty_metadata()).unwrap();
        found
            .problems
            .into_iter()
            .filter(|p| p.contains(".connection()"))
            .collect()
    }

    #[test]
    fn connection_in_a_service_crate_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/src/svc.rs",
            "fn f(tx: T) {\n    tx.connection().execute(\"x\", []);\n}\n",
        );
        let found = hits(dir.path());
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found
                .first()
                .is_some_and(|p| p.starts_with("crates/teamwire-a/src/svc.rs:2:"))
        );
    }

    #[test]
    fn cfg_test_on_a_const_does_not_hide_later_production_code() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/src/svc.rs",
            "#[cfg(test)]\nconst ALL: u8 = 1;\nfn f(tx: T) {\n    tx.connection();\n}\n",
        );
        let found = hits(dir.path());
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn test_crates_src_is_test_code() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\", \"tests/crash\", \"xtask\"]\n\n[workspace.package]\nversion = \"0.0.0\"\npublish = false\n",
        );
        write(
            dir.path(),
            "tests/crash/Cargo.toml",
            &std::fs::read_to_string(dir.path().join("crates/teamwire-a/Cargo.toml"))
                .unwrap()
                .replace("teamwire-a", "teamwire-crash"),
        );
        write(
            dir.path(),
            "tests/crash/src/lib.rs",
            "//! Purpose.\n//!\n//! Never: anything.\nfn f(t: T) { t.connection(); }\n",
        );
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        let found: Vec<_> = found
            .problems
            .iter()
            .filter(|p| p.contains(".connection()"))
            .collect();
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn connection_in_the_store_or_in_test_code_is_fine() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-store/Cargo.toml",
            &std::fs::read_to_string(dir.path().join("crates/teamwire-a/Cargo.toml"))
                .unwrap()
                .replace("teamwire-a", "teamwire-store"),
        );
        write(
            dir.path(),
            "crates/teamwire-store/src/lib.rs",
            "//! Purpose.\n//!\n//! Never: anything.\nfn f(t: T) { t.connection(); }\n",
        );
        write(
            dir.path(),
            "crates/teamwire-a/src/svc.rs",
            "// tx.connection() in a comment\nfn ok() {}\n#[cfg(test)]\nmod tests {\n    fn t(tx: T) { tx.connection(); }\n}\n",
        );
        write(
            dir.path(),
            "crates/teamwire-a/src/tests.rs",
            "fn t(tx: T) { tx.connection(); }\n",
        );
        let found = hits(dir.path());
        assert!(found.is_empty(), "{found:?}");
    }
}
