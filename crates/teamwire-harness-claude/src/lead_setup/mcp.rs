//! The lead's MCP server entry that ADR 0012 put in Claude Code's user
//! configuration, which install now removes (ADR 0018): reading it from
//! `.claude.json` and the `claude mcp` command that removes it.
//! Claude Code rewrites `.claude.json` from every running session, so this
//! module only reads that file and leaves every write to `claude mcp`.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;
use teamwire_core::Command;

use super::LeadSetupError;
use crate::ClaudeConfig;

/// The name of the entry, in `mcpServers` and on the `claude mcp` command line.
pub(crate) const NAME: &str = "teamwire";

/// A stdio MCP server entry: the program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpEntry {
    pub command: String,
    pub args: Vec<String>,
}

impl McpEntry {
    /// The entry the lead setup registers: `<teamwire_bin> [--home DIR] mcp
    /// --lead`. `home` is given only when the state directory is not the
    /// default, so a default install keeps following the default.
    pub fn lead(teamwire_bin: &Path, home: Option<&Path>) -> Self {
        let mut args = Vec::new();
        if let Some(home) = home {
            // Rebuilt from its components so `/x/` and `/x` give one entry,
            // and a second install does not see a different one.
            let home: PathBuf = home.components().collect();
            args.push("--home".to_owned());
            args.push(home.display().to_string());
        }
        args.push("mcp".to_owned());
        args.push("--lead".to_owned());
        Self {
            command: teamwire_bin.display().to_string(),
            args,
        }
    }

    /// The arguments before `mcp --lead`: `--home DIR` or nothing. The lead
    /// plugin's hook command takes the same global arguments.
    pub(crate) fn home_args(&self) -> &[String] {
        self.args
            .get(..self.args.len().saturating_sub(2))
            .unwrap_or_default()
    }

    /// True for any `teamwire [--home DIR] mcp --lead`, whichever binary
    /// path and state directory it names.
    fn is_teamwire_lead(&self) -> bool {
        let program_is_teamwire = Path::new(&self.command)
            .file_name()
            .is_some_and(|n| n == "teamwire");
        let args: Vec<&str> = self.args.iter().map(String::as_str).collect();
        let args_match = matches!(
            args.as_slice(),
            ["mcp", "--lead"] | ["--home", _, "mcp", "--lead"]
        );
        program_is_teamwire && args_match
    }
}

impl fmt::Display for McpEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.command)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

/// What the user-scope `teamwire` entry is, compared with the one wanted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpState {
    Missing,
    Current,
    /// A `teamwire [--home DIR] mcp --lead` entry with another binary path or
    /// state directory.
    Outdated(McpEntry),
    /// An entry named `teamwire` that is not the lead's server, described.
    Foreign(String),
}

/// A flag whose name (`--token`, `-api-key`) marks its value as a secret.
fn is_secret_flag(name: &str) -> bool {
    name.starts_with('-') && teamwire_core::is_secret_name(name)
}

/// `word` with a URL's user info, query and fragment removed, since each can
/// hold a credential. A word that is not a URL is returned as it is.
fn without_url_secrets(word: &str) -> String {
    let Some((scheme, rest)) = word.split_once("://") else {
        return word.to_owned();
    };
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{path}")
}

/// An entry in words for a refusal or a `doctor` line, which people paste
/// into issues: the type, the program or URL and the arguments, but never
/// an environment value, a header, the value of a secret flag (after it or
/// after its `=`), or a URL's user info or query.
pub(crate) fn describe(entry: &Value) -> String {
    let text = |k: &str| entry.get(k).and_then(Value::as_str);
    let mut words: Vec<String> = Vec::new();
    if let Some(kind) = text("type").filter(|t| *t != "stdio") {
        words.push(format!("({kind})"));
    }
    words.extend(text("command").or(text("url")).map(without_url_secrets));
    let mut hide_next = false;
    for arg in entry
        .get("args")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let arg = arg.as_str().unwrap_or("?");
        let word = match arg.split_once('=') {
            _ if hide_next => "<hidden>".to_owned(),
            Some((name, _)) if is_secret_flag(name) => format!("{name}=<hidden>"),
            _ => without_url_secrets(arg),
        };
        words.push(word);
        hide_next = !arg.contains('=') && is_secret_flag(arg);
    }
    for (key, noun) in [("env", "environment variable"), ("headers", "header")] {
        let n = entry
            .get(key)
            .and_then(Value::as_object)
            .map_or(0, serde_json::Map::len);
        if n > 0 {
            words.push(format!("with {n} {noun}{}", if n == 1 { "" } else { "s" }));
        }
    }
    if words.is_empty() {
        "an entry with no command or URL".to_owned()
    } else {
        words.join(" ")
    }
}

/// What the parsed entry is. An entry with its own environment variables
/// is someone's hand-made variant, so it counts as foreign.
pub(crate) fn classify(entry: &Value, want: &McpEntry) -> McpState {
    let kind_is_stdio = entry
        .get("type")
        .is_none_or(|t| t.as_str() == Some("stdio"));
    let env_is_empty = entry
        .get("env")
        .is_none_or(|e| e.as_object().is_some_and(serde_json::Map::is_empty));
    let command = entry.get("command").and_then(Value::as_str);
    let args: Option<Vec<String>> = match entry.get("args") {
        None => Some(Vec::new()),
        Some(v) => v.as_array().and_then(|a| {
            a.iter()
                .map(|s| s.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        }),
    };
    let found = match (command, args) {
        (Some(command), Some(args)) if kind_is_stdio && env_is_empty => McpEntry {
            command: command.to_owned(),
            args,
        },
        _ => return McpState::Foreign(describe(entry)),
    };
    if &found == want {
        McpState::Current
    } else if found.is_teamwire_lead() {
        McpState::Outdated(found)
    } else {
        McpState::Foreign(describe(entry))
    }
}

/// The parsed JSON file at `path`, or `None` when there is no file.
pub(crate) fn read_json(path: &Path) -> Result<Option<Value>, LeadSetupError> {
    let failed = |detail: String| LeadSetupError::Read {
        path: path.to_path_buf(),
        detail,
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failed(e.to_string())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| failed(e.to_string()))
}

/// The `teamwire` entry of an `mcpServers` map inside `doc`.
pub(crate) fn entry_in(doc: &Value) -> Option<&Value> {
    doc.get("mcpServers").and_then(|s| s.get(NAME))
}

/// The state of the user-scope `teamwire` entry in `claude_json`. A missing
/// file is a configuration with no servers yet.
pub(crate) fn read(claude_json: &Path, want: &McpEntry) -> Result<McpState, LeadSetupError> {
    let Some(doc) = read_json(claude_json)? else {
        return Ok(McpState::Missing);
    };
    Ok(match entry_in(&doc) {
        Some(entry) => classify(entry, want),
        None => McpState::Missing,
    })
}

/// A `claude` command run against the configuration in `config`. When
/// Claude's directory also holds `.claude.json`, that layout is Claude's
/// `CLAUDE_CONFIG_DIR` layout, so the variable is set and `claude` writes
/// where this module reads. The default layout keeps `.claude.json` beside
/// `~/.claude`, and there the variable must stay unset.
pub(crate) fn claude(config: &ClaudeConfig) -> Command {
    let cmd = Command::new(config.claude_bin.clone());
    if config.claude_json == config.claude_dir.join(".claude.json") {
        cmd.env("CLAUDE_CONFIG_DIR", config.claude_dir.display().to_string())
    } else {
        cmd
    }
}

pub(crate) fn remove_command(config: &ClaudeConfig) -> Command {
    claude(config).args(["mcp", "remove", "--scope", "user", NAME])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn want() -> McpEntry {
        McpEntry::lead(Path::new("/opt/bin/teamwire"), None)
    }

    #[test]
    fn the_lead_entry_names_home_only_when_given() {
        assert_eq!(want().args, ["mcp", "--lead"]);
        let e = McpEntry::lead(Path::new("/b/teamwire"), Some(Path::new("/h")));
        assert_eq!(e.args, ["--home", "/h", "mcp", "--lead"]);
        assert_eq!(e.to_string(), "/b/teamwire --home /h mcp --lead");
    }

    #[test]
    fn a_trailing_slash_in_home_gives_the_same_entry() {
        let bin = Path::new("/b/teamwire");
        assert_eq!(
            McpEntry::lead(bin, Some(Path::new("/scratch/tw/"))),
            McpEntry::lead(bin, Some(Path::new("/scratch/tw")))
        );
    }

    #[test]
    fn a_foreign_entry_is_described_without_its_secrets() {
        let text = describe(&json!({
            "command": "teamwire",
            "args": ["mcp", "--team", "t", "--token", "s3cret"],
            "env": {"TEAMWIRE_TOKEN": "s3cret2"}
        }));
        assert_eq!(
            text,
            "teamwire mcp --team t --token <hidden> with 1 environment variable"
        );
        let http = describe(&json!({
            "type": "http", "url": "https://x.example/mcp",
            "headers": {"Authorization": "Bearer s3cret3"}
        }));
        assert_eq!(http, "(http) https://x.example/mcp with 1 header");
        assert!(!format!("{text}{http}").contains("s3cret"));
    }

    #[test]
    fn a_secret_inside_one_argument_or_a_url_is_hidden() {
        let text = describe(&json!({
            "command": "server",
            "args": ["--token=abc", "--team=t", "https://u:p@h/x?key=s", "--api-key", "k"]
        }));
        assert_eq!(
            text,
            "server --token=<hidden> --team=t https://h/x --api-key <hidden>"
        );
        let http = describe(&json!({"type": "http", "url": "https://u:p@h/x?key=s#f"}));
        assert_eq!(http, "(http) https://h/x");
        assert_eq!(without_url_secrets("https://h?q=1"), "https://h");
    }

    #[test]
    fn the_wanted_entry_is_current() {
        let entry = json!({"type": "stdio", "command": "/opt/bin/teamwire", "args": ["mcp", "--lead"], "env": {}});
        assert_eq!(classify(&entry, &want()), McpState::Current);
    }

    #[test]
    fn a_teamwire_lead_entry_on_path_or_another_home_is_outdated() {
        for entry in [
            json!({"command": "teamwire", "args": ["mcp", "--lead"]}),
            json!({"type": "stdio", "command": "/old/teamwire", "args": ["--home", "/x", "mcp", "--lead"], "env": {}}),
        ] {
            assert!(
                matches!(classify(&entry, &want()), McpState::Outdated(_)),
                "{entry}"
            );
        }
    }

    #[test]
    fn anything_else_named_teamwire_is_foreign() {
        for entry in [
            json!({"command": "teamwire", "args": ["mcp", "--team", "t", "--role", "r"]}),
            json!({"command": "/bin/other", "args": ["mcp", "--lead"]}),
            json!({"command": "teamwire", "args": ["mcp", "--lead"], "env": {"TEAMWIRE_HOME": "/x"}}),
            json!({"type": "http", "url": "https://example.com/mcp"}),
        ] {
            assert!(
                matches!(classify(&entry, &want()), McpState::Foreign(_)),
                "{entry}"
            );
        }
    }

    #[test]
    fn claude_config_dir_is_set_only_for_the_layout_that_needs_it() {
        let default = ClaudeConfig {
            claude_dir: PathBuf::from("/u/.claude"),
            claude_json: PathBuf::from("/u/.claude.json"),
            claude_bin: "claude".to_owned(),
        };
        assert!(remove_command(&default).env.is_empty());
        let dir = ClaudeConfig {
            claude_dir: PathBuf::from("/c"),
            claude_json: PathBuf::from("/c/.claude.json"),
            claude_bin: "claude".to_owned(),
        };
        assert_eq!(
            remove_command(&dir).env,
            [("CLAUDE_CONFIG_DIR".to_owned(), "/c".to_owned())]
        );
        assert_eq!(
            remove_command(&dir).args,
            ["mcp", "remove", "--scope", "user", "teamwire"]
        );
    }

    #[test]
    fn home_args_are_what_comes_before_mcp_lead() {
        assert!(want().home_args().is_empty());
        let e = McpEntry::lead(Path::new("/b/teamwire"), Some(Path::new("/h")));
        assert_eq!(e.home_args(), ["--home", "/h"]);
    }
}
