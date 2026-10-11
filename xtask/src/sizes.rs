//! File-size rule of `docs/conventions.md` section 1: a source file over 400
//! non-test lines is reported, over 800 fails.
//!
//! Never: count a `#[cfg(test)]` module, a `tests.rs` file or a file under
//! a `tests/` directory (integration tests, or `src/<file>/tests/`).
//! Invariants: rustfmt closes a test module with a `}` in column 0, which is
//! how the end of the module is found.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::code_text::code_only;
use crate::{Findings, relative};

const REPORT_OVER: usize = 400;
const FAIL_OVER: usize = 800;

pub(crate) fn check_sizes(root: &Path, findings: &mut Findings) -> Result<()> {
    let mut files = Vec::new();
    collect(root, &mut files)?;
    files.sort();
    for path in files {
        if in_integration_tests(root, &path) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let lines = non_test_lines(&text);
        let name = relative(root, &path);
        if lines > FAIL_OVER {
            findings.problems.push(format!(
                "{name}: {lines} non-test lines; split it, the limit is {FAIL_OVER}"
            ));
        } else if lines > REPORT_OVER {
            findings.notes.push(format!(
                "{name}: {lines} non-test lines; consider splitting past {REPORT_OVER}"
            ));
        }
    }
    Ok(())
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !matches!(file_name, "target" | ".git" | ".worktrees") {
                collect(&path, files)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// Test code by location: `<crate>/tests/**`, `src/**/tests.rs` and
/// `src/**/tests/*.rs`.
fn in_integration_tests(root: &Path, path: &Path) -> bool {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == root {
            return false;
        }
        if d.join("Cargo.toml").is_file() {
            return path.strip_prefix(d).is_ok_and(|p| {
                p.file_name().is_some_and(|n| n == "tests.rs")
                    || p.components().any(|c| c.as_os_str() == "tests")
            });
        }
        dir = d.parent();
    }
    false
}

/// Lines outside `#[cfg(test)]` items. The skipped item ends where its braces
/// balance, or at a `;` when it has no body, so a test-only method inside an
/// `impl` does not hide the rest of the `impl`.
fn non_test_lines(text: &str) -> usize {
    let mut count = 0;
    let mut skipping = false;
    let mut depth = 0_i64;
    let mut opened = false;
    // Braces in a char or string literal or a comment must not move the depth.
    for line in code_only(text).lines() {
        if skipping {
            let opens = line.matches('{').count();
            let closes = line.matches('}').count();
            opened |= opens > 0;
            depth += i64::try_from(opens).unwrap_or(0) - i64::try_from(closes).unwrap_or(0);
            let bodiless = !opened && line.trim_end().ends_with(';');
            if bodiless || (opened && depth <= 0) {
                skipping = false;
            }
        } else if line.trim() == "#[cfg(test)]" {
            skipping = true;
            depth = 0;
            opened = false;
        } else {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use crate::fixture::{NOTE, clean_workspace, empty_metadata, write};
    use crate::tidy;

    fn source(lines: usize) -> String {
        format!("{NOTE}{}", "let x = 1;\n".repeat(lines - 3))
    }

    #[test]
    fn file_of_400_lines_is_silent_and_401_is_reported_not_failed() {
        let dir = clean_workspace();
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &source(400));
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(
            found.problems.is_empty() && found.notes.is_empty(),
            "{found:?}"
        );
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &source(401));
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(found.problems.is_empty(), "{found:?}");
        assert_eq!(found.notes.len(), 1);
        assert!(
            found
                .notes
                .first()
                .is_some_and(|n| n.contains("401 non-test lines"))
        );
    }

    #[test]
    fn file_of_801_non_test_lines_fails() {
        let dir = clean_workspace();
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &source(800));
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(found.problems.is_empty(), "{found:?}");
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &source(801));
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert_eq!(found.problems.len(), 1, "{found:?}");
        assert!(
            found
                .problems
                .first()
                .is_some_and(|p| p.contains("801 non-test lines"))
        );
    }

    #[test]
    fn test_module_lines_do_not_count() {
        let dir = clean_workspace();
        let text = format!(
            "{}\n#[cfg(test)]\nmod tests {{\n{}}}\n",
            source(300),
            "    let y = 2;\n".repeat(900)
        );
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &text);
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(
            found.problems.is_empty() && found.notes.is_empty(),
            "{found:?}"
        );
    }

    #[test]
    fn test_method_inside_an_impl_does_not_hide_the_rest_of_the_impl() {
        let dir = clean_workspace();
        let text = format!(
            "{}\nimpl Thing {{\n    #[cfg(test)]\n    fn helper() {{\n        let y = 2;\n    }}\n{}}}\n",
            source(300),
            "    fn real() {}\n".repeat(101)
        );
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &text);
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert_eq!(found.notes.len(), 1, "{found:?}");
    }

    #[test]
    fn braces_in_literals_inside_a_skipped_item_do_not_extend_it() {
        let dir = clean_workspace();
        let text = format!(
            "{}\n#[cfg(test)]\nfn helper() {{\n    let open = '{{'; // {{\n}}\n{}",
            source(300),
            "fn real() {}\n".repeat(101)
        );
        write(dir.path(), "crates/teamwire-a/src/lib.rs", &text);
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert_eq!(found.notes.len(), 1, "{found:?}");
    }

    #[test]
    fn sibling_test_files_under_src_do_not_count() {
        let dir = clean_workspace();
        let big = "let z = 3;\n".repeat(900);
        write(dir.path(), "crates/teamwire-a/src/thing/tests.rs", &big);
        write(
            dir.path(),
            "crates/teamwire-a/src/thing/tests/more.rs",
            &big,
        );
        write(dir.path(), "crates/teamwire-a/src/tests.rs", &big);
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(
            found.problems.is_empty() && found.notes.is_empty(),
            "{found:?}"
        );
        write(dir.path(), "crates/teamwire-a/src/thing.rs", &big);
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert_eq!(found.problems.len(), 1, "{found:?}");
    }

    #[test]
    fn integration_test_files_do_not_count() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-a/tests/big.rs",
            &"let z = 3;\n".repeat(900),
        );
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(
            found.problems.is_empty() && found.notes.is_empty(),
            "{found:?}"
        );
    }
}
