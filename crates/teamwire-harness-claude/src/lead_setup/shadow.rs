//! `teamwire` MCP entries that Claude Code prefers over the user-scope one:
//! the local scope (`projects.<dir>.mcpServers` in `.claude.json`) and the
//! project scope (`<dir>/.mcp.json`). Claude Code starts the local entry
//! first, then the project one, then the user one, so a leftover entry in
//! either runs in place of what `teamwire install` registered.

use std::path::Path;

use super::LeadSetupError;
use super::mcp::{self, McpEntry, McpState};
use crate::ClaudeConfig;

/// A `teamwire` entry that hides the user-scope one in a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadow {
    /// Where the entry is, in words.
    pub place: String,
    /// The entry, described without secrets.
    pub entry: String,
}

/// The entries that hide the user-scope one for a Claude session started
/// in `dir`. An entry identical to `want` is not reported: it starts the
/// same server.
pub fn shadowing_entries(
    config: &ClaudeConfig,
    dir: &Path,
    want: &McpEntry,
) -> Result<Vec<Shadow>, LeadSetupError> {
    let mut found = Vec::new();
    let mut note = |place: String, entry: &serde_json::Value| {
        if mcp::classify(entry, want) != McpState::Current {
            found.push(Shadow {
                place,
                entry: mcp::describe(entry),
            });
        }
    };
    if let Some(doc) = mcp::read_json(&config.claude_json)?
        && let Some(entry) = doc
            .get("projects")
            .and_then(|p| p.get(dir.display().to_string()))
            .and_then(mcp::entry_in)
    {
        note(
            format!(
                "the local scope for {} in {}",
                dir.display(),
                config.claude_json.display()
            ),
            entry,
        );
    }
    let project = dir.join(".mcp.json");
    if let Some(doc) = mcp::read_json(&project)?
        && let Some(entry) = mcp::entry_in(&doc)
    {
        note(project.display().to_string(), entry);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[expect(
        clippy::disallowed_methods,
        reason = "the test writes Claude's files as Claude Code would"
    )]
    fn write(path: &Path, value: &serde_json::Value) {
        std::fs::write(path, value.to_string()).unwrap();
    }

    #[test]
    fn local_and_project_entries_that_differ_are_reported_and_an_identical_one_is_not() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        let config = ClaudeConfig {
            claude_dir: tmp.path().to_path_buf(),
            claude_json: tmp.path().join(".claude.json"),
            claude_bin: "claude".to_owned(),
        };
        let want = McpEntry::lead(Path::new("/opt/bin/teamwire"), None);
        assert_eq!(shadowing_entries(&config, &dir, &want).unwrap(), []);

        let path_form =
            json!({"mcpServers": {"teamwire": {"command": "teamwire", "args": ["mcp", "--lead"]}}});
        write(&dir.join(".mcp.json"), &path_form);
        write(
            &config.claude_json,
            &json!({"projects": {dir.display().to_string(): path_form}}),
        );
        let found = shadowing_entries(&config, &dir, &want).unwrap();
        assert_eq!(found.len(), 2, "{found:?}");
        let [local, project] = found.as_slice() else {
            unreachable!("two entries: {found:?}")
        };
        assert!(local.place.starts_with("the local scope"), "{local:?}");
        assert_eq!(project.place, dir.join(".mcp.json").display().to_string());
        assert_eq!(project.entry, "teamwire mcp --lead");

        let same = json!({"mcpServers": {"teamwire": {"command": "/opt/bin/teamwire", "args": ["mcp", "--lead"]}}});
        write(&dir.join(".mcp.json"), &same);
        let found = shadowing_entries(&config, &dir, &want).unwrap();
        assert_eq!(found.len(), 1, "an identical project entry is harmless");
    }
}
