//! The adapter rule of `docs/conventions.md` section 5: `teamwire-cli`
//! names the layer-2 adapter crates only in `src/wiring.rs`.
//!
//! Never: change a file. Invariants: each other file under
//! `crates/teamwire-cli/src/` that names an adapter crate adds one line
//! naming the file, the line and the crate.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::code_text::code_only;
use crate::relative;

const ADAPTERS: &[&str] = &[
    "teamwire_host_herdr",
    "teamwire_host_tmux",
    "teamwire_harness_claude",
    "teamwire_harness_codex",
];
const CLI_SRC: &str = "crates/teamwire-cli/src";
const WIRING: &str = "crates/teamwire-cli/src/wiring.rs";

pub(crate) fn check_adapters(root: &Path, problems: &mut Vec<String>) -> Result<()> {
    let mut files = Vec::new();
    rust_files(&root.join(CLI_SRC), &mut files)?;
    files.sort();
    for path in files {
        let name = relative(root, &path);
        if name == WIRING {
            continue;
        }
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {name}"))?;
        for (index, line) in code_only(&text).lines().enumerate() {
            if let Some(adapter) = ADAPTERS.iter().find(|a| line.contains(**a)) {
                problems.push(format!(
                    "{name}:{}: names {adapter}; adapter crates are named only in wiring.rs (docs/conventions.md section 5)",
                    index + 1
                ));
            }
        }
    }
    Ok(())
}

/// Every `.rs` file under `dir`; a missing directory holds none.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::fixture::{clean_workspace, empty_metadata, write};
    use crate::tidy;

    #[test]
    fn adapter_named_outside_wiring_fails() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-cli/src/wiring.rs",
            "use teamwire_host_tmux::Tmux;\n",
        );
        write(
            dir.path(),
            "crates/teamwire-cli/src/cmd/hook.rs",
            "// teamwire_host_tmux in a comment is fine\nuse teamwire_harness_claude::ClaudeCode;\n",
        );
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        let hits: Vec<_> = found
            .problems
            .iter()
            .filter(|p| p.contains("adapter crates"))
            .collect();
        assert_eq!(hits.len(), 1, "{:?}", found.problems);
        assert!(
            hits.first()
                .is_some_and(|p| p.starts_with("crates/teamwire-cli/src/cmd/hook.rs:2:"))
        );
    }

    #[test]
    fn adapter_named_in_a_string_or_a_comment_is_fine() {
        let dir = clean_workspace();
        write(
            dir.path(),
            "crates/teamwire-cli/src/cmd/hook.rs",
            "let a = \"teamwire_host_tmux\";\nlet b = 1; // teamwire_host_tmux\n/* teamwire_host_herdr\n teamwire_harness_claude */\nlet c = r#\"teamwire_host_tmux\"#;\n",
        );
        let found = tidy(dir.path(), &empty_metadata()).unwrap();
        assert!(
            !found.problems.iter().any(|p| p.contains("adapter crates")),
            "{:?}",
            found.problems
        );
    }
}
