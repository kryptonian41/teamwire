//! Fixture workspaces for the tests of every rule.

use std::path::Path;

use serde_json::{Value, json};

pub(crate) fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "test fixtures in a temporary directory"
    )]
    std::fs::write(full, text).unwrap();
}

pub(crate) const NOTE: &str = "//! Purpose.\n//!\n//! Never: anything.\n";

pub(crate) fn empty_metadata() -> Value {
    json!({"packages": []})
}

/// A workspace that passes every rule: one crate, `teamwire-a`, and xtask.
pub(crate) fn clean_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\", \"xtask\"]\n\n[workspace.package]\nversion = \"0.0.0\"\npublish = false\n",
    );
    write(root, "CLAUDE.md", "@AGENTS.md\n");
    write(root, "AGENTS.md", "# rules\n");
    write(
        root,
        "docs/ARCHITECTURE.md",
        "# x\n\n## Components\n\n| Crate | What it does |\n| --- | --- |\n| `teamwire-a` | Does a. |\n| `tests/fakes` (`teamwire-fakes`) | Fakes. |\n\n## Next\n\n| `teamwire-ghost` | not in the section |\n",
    );
    let good = "[package]\nname = \"NAME\"\nversion.workspace = true\npublish.workspace = true\n\n[dependencies]\nserde = { workspace = true }\n\n[lints]\nworkspace = true\n";
    write(
        root,
        "crates/teamwire-a/Cargo.toml",
        &good.replace("NAME", "teamwire-a"),
    );
    write(root, "crates/teamwire-a/src/lib.rs", NOTE);
    write(root, "xtask/Cargo.toml", &good.replace("NAME", "xtask"));
    write(root, "xtask/src/main.rs", NOTE);
    dir
}
