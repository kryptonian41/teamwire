//! `claude mcp add --scope user` and `claude mcp remove --scope user`: the
//! user-scope `mcpServers` map of `<dir>/.claude.json`, with Claude Code's
//! refusals (exit 1 for a name that exists on add, or is missing on
//! remove). `FAKE_CLAUDE_MCP_FAIL=add` or `=remove` makes that command fail
//! with exit 1, so a test can stop an install partway through.

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::fsutil;

/// Runs `claude mcp ...`. `claude_dir` is where the fake keeps Claude's
/// files; teamwire must name the same directory in `CLAUDE_CONFIG_DIR`, as
/// the real `claude` writes there.
pub(super) fn run(claude_dir: &Path, argv: &[String]) -> ExitCode {
    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    if config_dir.as_deref() != Some(claude_dir.as_os_str()) {
        return super::refuse(&format!(
            "CLAUDE_CONFIG_DIR is {config_dir:?}, not {}; the real claude would write elsewhere",
            claude_dir.display()
        ));
    }
    let action = argv.get(1).map(String::as_str);
    if !argv.windows(2).any(|w| w == ["--scope", "user"]) {
        return super::refuse("only `--scope user` is implemented");
    }
    if std::env::var("FAKE_CLAUDE_MCP_FAIL").ok().as_deref() == action {
        return fail("FAKE_CLAUDE_MCP_FAIL asked this command to fail");
    }
    let path = claude_dir.join(".claude.json");
    let mut doc: Value = fsutil::read_text(&path)
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}));
    let positional: Vec<&String> = argv
        .iter()
        .skip(2)
        .take_while(|a| a.as_str() != "--")
        .filter(|a| !a.starts_with('-') && a.as_str() != "user")
        .collect();
    let Some(name) = positional.first().map(|s| s.as_str()) else {
        return super::refuse("no server name");
    };
    let Some(servers) = doc
        .as_object_mut()
        .map(|o| o.entry("mcpServers").or_insert_with(|| json!({})))
        .and_then(Value::as_object_mut)
    else {
        return super::refuse(".claude.json is not an object");
    };
    match action {
        Some("add") => {
            if servers.contains_key(name) {
                return fail(&format!("MCP server {name} already exists in user config"));
            }
            let mut rest = argv.iter().skip_while(|a| a.as_str() != "--").skip(1);
            let Some(command) = rest.next() else {
                return super::refuse("no command after --");
            };
            let args: Vec<&String> = rest.collect();
            servers.insert(
                name.to_owned(),
                json!({"type": "stdio", "command": command, "args": args, "env": {}}),
            );
        }
        Some("remove") => {
            if servers.remove(name).is_none() {
                return fail(&format!("No MCP server named \"{name}\" in user scope"));
            }
        }
        _ => return super::refuse("only `mcp add` and `mcp remove` are implemented"),
    }
    if let Err(e) = fsutil::write_atomic(&path, doc.to_string().as_bytes()) {
        return super::refuse(&format!("cannot write {}: {e}", path.display()));
    }
    super::say(&format!("File modified: {}", path.display()));
    ExitCode::SUCCESS
}

/// A refusal Claude Code itself makes: exit 1, the reason on stderr.
fn fail(message: &str) -> ExitCode {
    use std::io::Write as _;
    writeln!(std::io::stderr().lock(), "{message}").ok();
    ExitCode::from(1)
}
