//! The teamwire plugin (ADR 0018): the files `teamwire install` renders under
//! `$TEAMWIRE_HOME/claude-plugin/`, what Claude Code has registered from them,
//! and the `claude plugin` commands that register and remove them. Claude
//! Code keeps its own plugin files, so this module only reads them and leaves
//! every write there to `claude plugin`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use teamwire_core::{Command, LaunchSpec};

use super::mcp::{McpEntry, claude};
use super::skill;
use super::skill::{AFK_TEXT, PROMPTWIRE_TEXT, REFERENCE_TEXT};
use super::{LeadSetupError, SKILL_TEXT};
use crate::ClaudeConfig;

/// The marketplace and the plugin in it, both named `teamwire`.
pub(crate) const MARKETPLACE: &str = "teamwire";
pub(crate) use crate::launch::LEAD_PLUGIN_ID as PLUGIN_ID;

/// The hook timeout in seconds. The hook only reads local files, so it stays
/// well under the time a person waits for a turn to end.
const STOP_HOOK_TIMEOUT_S: u64 = 10;

/// The plugin's directory inside the marketplace.
const PLUGIN_DIR: &str = "teamwire";

/// The marketplace directory's files, as this binary renders them: each path
/// is relative to the directory, with its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginFiles {
    pub(crate) dir: PathBuf,
    pub(crate) files: Vec<(PathBuf, String)>,
}

/// `<home>/claude-plugin`: the marketplace directory.
pub(crate) fn plugin_dir(home: &Path) -> PathBuf {
    home.join("claude-plugin")
}

fn pretty(doc: &Value) -> String {
    // A `json!` value always serializes; the fallback keeps the function total.
    serde_json::to_string_pretty(doc).map_or_else(|_| doc.to_string(), |t| t + "\n")
}

impl PluginFiles {
    /// The files for the marketplace at `dir`, running `want`'s binary with
    /// its `--home`. Fails when the compiled-in skill has no frontmatter.
    pub(crate) fn render(dir: &Path, want: &McpEntry) -> Result<Self, String> {
        Self::render_with(dir, want, REFERENCE_TEXT)
    }

    /// `render` with the text of `reference.md` given, so a test can change it
    /// alone.
    fn render_with(dir: &Path, want: &McpEntry, reference: &str) -> Result<Self, String> {
        let skill = skill::stamp(SKILL_TEXT)
            .ok_or_else(|| "the compiled-in skill has no frontmatter".to_owned())?;
        let afk = skill::stamp(AFK_TEXT)
            .ok_or_else(|| "the compiled-in afk skill has no frontmatter".to_owned())?;
        let promptwire = skill::stamp(PROMPTWIRE_TEXT)
            .ok_or_else(|| "the compiled-in promptwire skill has no frontmatter".to_owned())?;
        let mut hook_words = vec![LaunchSpec::shell_word(&want.command)];
        hook_words.extend(want.home_args().iter().map(|a| LaunchSpec::shell_word(a)));
        hook_words.extend(["hook".to_owned(), "claude-lead-stop".to_owned()]);
        let body = [
            (
                format!("{PLUGIN_DIR}/.mcp.json"),
                pretty(&json!({"mcpServers": {"teamwire": {
                    "command": want.command,
                    "args": want.args,
                }}})),
            ),
            (
                format!("{PLUGIN_DIR}/hooks/hooks.json"),
                pretty(&json!({"hooks": {"Stop": [{"hooks": [{
                    "type": "command",
                    "command": hook_words.join(" "),
                    "timeout": STOP_HOOK_TIMEOUT_S,
                }]}]}})),
            ),
            (format!("{PLUGIN_DIR}/skills/teamwire/SKILL.md"), skill),
            (
                format!("{PLUGIN_DIR}/skills/teamwire/reference.md"),
                reference.to_owned(),
            ),
            (format!("{PLUGIN_DIR}/skills/afk/SKILL.md"), afk),
            (
                format!("{PLUGIN_DIR}/skills/promptwire/SKILL.md"),
                promptwire,
            ),
        ];
        // The version changes with the content, so `claude plugin update`
        // sees a new binary or skill as a new version and refreshes its copy.
        let mut content = String::new();
        for (path, text) in &body {
            content.push_str(path);
            content.push('\0');
            content.push_str(text);
            content.push('\0');
        }
        let version: String = skill::sha256_hex(&content).chars().take(12).collect();
        let manifest = (
            format!("{PLUGIN_DIR}/.claude-plugin/plugin.json"),
            pretty(&json!({
                "name": MARKETPLACE,
                "version": version,
                "description": "Lead tools, skill and Stop hook for teamwire teams",
            })),
        );
        let marketplace = (
            ".claude-plugin/marketplace.json".to_owned(),
            pretty(&json!({
                "name": MARKETPLACE,
                "owner": {"name": "teamwire"},
                "plugins": [{
                    "name": MARKETPLACE,
                    "source": format!("./{PLUGIN_DIR}"),
                    "description": "Lead tools, skill and Stop hook for teamwire teams",
                }],
            })),
        );
        Ok(Self {
            dir: dir.to_path_buf(),
            files: std::iter::once(marketplace)
                .chain(std::iter::once(manifest))
                .chain(body)
                .map(|(path, text)| (PathBuf::from(path), text))
                .collect(),
        })
    }

    /// True when every file is on disk at `dir` with this text.
    pub(crate) fn is_written(&self) -> bool {
        self.files.iter().all(|(path, text)| {
            std::fs::read_to_string(self.dir.join(path)).is_ok_and(|t| &t == text)
        })
    }

    /// True when the plugin's own files under `root` (the directory Claude
    /// loads the plugin from) hold this text. The marketplace file is not part
    /// of the plugin, so it is not compared.
    pub(crate) fn plugin_matches(&self, root: &Path) -> bool {
        self.files
            .iter()
            .filter_map(|(path, text)| Some((path.strip_prefix(PLUGIN_DIR).ok()?, text)))
            .all(|(rel, text)| std::fs::read_to_string(root.join(rel)).is_ok_and(|t| &t == text))
    }
}

/// Where the user's `teamwire` marketplace came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarketplaceState {
    Missing,
    /// Registered from this marketplace directory, or from a source this
    /// module cannot tell apart from it.
    Ours,
    /// Registered from some other source.
    Foreign,
}

/// What Claude Code has registered from the plugin, at user scope. Read from
/// Claude's own files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginState {
    pub(crate) marketplace: MarketplaceState,
    /// The directory Claude loads the plugin from.
    pub(crate) installed: Option<PathBuf>,
    /// The user-scope `enabledPlugins` value, when it has one.
    pub(crate) enabled: Option<bool>,
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

fn marketplace_state(
    config: &ClaudeConfig,
    dir: &Path,
) -> Result<MarketplaceState, LeadSetupError> {
    let path = config
        .claude_dir
        .join("plugins")
        .join("known_marketplaces.json");
    let Some(entry) = super::mcp::read_json(&path)?.and_then(|d| d.get(MARKETPLACE).cloned())
    else {
        return Ok(MarketplaceState::Missing);
    };
    let source = entry.get("source");
    let kind = source.and_then(|s| s.get("source")).and_then(Value::as_str);
    let from = source.and_then(|s| s.get("path")).and_then(Value::as_str);
    Ok(match (kind, from) {
        (Some("directory"), Some(from)) if !same_dir(Path::new(from), dir) => {
            MarketplaceState::Foreign
        }
        (Some(kind), _) if kind != "directory" => MarketplaceState::Foreign,
        _ => MarketplaceState::Ours,
    })
}

/// What Claude Code has registered, for the marketplace at `dir`. Reads only.
pub(crate) fn plugin_state(
    config: &ClaudeConfig,
    dir: &Path,
) -> Result<PluginState, LeadSetupError> {
    let plugins = config
        .claude_dir
        .join("plugins")
        .join("installed_plugins.json");
    let installed = super::mcp::read_json(&plugins)?
        .and_then(|d| d.get("plugins")?.get(PLUGIN_ID)?.as_array().cloned())
        .into_iter()
        .flatten()
        .find(|e| e.get("scope").and_then(Value::as_str) == Some("user"))
        .and_then(|e| e.get("installPath")?.as_str().map(PathBuf::from));
    let enabled = super::mcp::read_json(&config.claude_dir.join("settings.json"))?
        .and_then(|d| d.get("enabledPlugins")?.get(PLUGIN_ID)?.as_bool());
    Ok(PluginState {
        marketplace: marketplace_state(config, dir)?,
        installed,
        enabled,
    })
}

/// `claude plugin --help`: proves this `claude` has the `plugin` command
/// before anything is written.
pub(crate) fn probe_command(config: &ClaudeConfig) -> Command {
    claude(config).args(["plugin", "--help"])
}

pub(crate) fn marketplace_add(config: &ClaudeConfig, dir: &Path) -> Command {
    claude(config)
        .args(["plugin", "marketplace", "add", "--scope", "user"])
        .arg(dir.display().to_string())
}

pub(crate) fn marketplace_update(config: &ClaudeConfig) -> Command {
    claude(config).args(["plugin", "marketplace", "update", MARKETPLACE])
}

pub(crate) fn marketplace_remove(config: &ClaudeConfig) -> Command {
    claude(config).args([
        "plugin",
        "marketplace",
        "remove",
        "--scope",
        "user",
        MARKETPLACE,
    ])
}

/// `claude plugin <verb> teamwire@teamwire --scope user`, for `install`,
/// `update`, `enable` and `uninstall`.
pub(crate) fn plugin_verb(config: &ClaudeConfig, verb: &str) -> Command {
    claude(config).args(["plugin", verb, PLUGIN_ID, "--scope", "user"])
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    reason = "the tests index JSON they just built and write Claude's files as Claude Code would"
)]
mod tests {
    use super::*;

    fn want(home: Option<&str>) -> McpEntry {
        McpEntry::lead(Path::new("/opt/bin/teamwire"), home.map(Path::new))
    }

    fn file<'a>(files: &'a PluginFiles, path: &str) -> &'a str {
        files
            .files
            .iter()
            .find(|(p, _)| p == Path::new(path))
            .map(|(_, text)| text.as_str())
            .unwrap_or_else(|| unreachable!("no file {path}"))
    }

    fn json_of(files: &PluginFiles, path: &str) -> Value {
        serde_json::from_str(file(files, path)).unwrap()
    }

    #[test]
    fn the_eight_files_name_the_binary_and_the_hook() {
        let files = PluginFiles::render(Path::new("/h/claude-plugin"), &want(None)).unwrap();
        let paths: Vec<_> = files
            .files
            .iter()
            .map(|(p, _)| p.display().to_string())
            .collect();
        assert_eq!(
            paths,
            [
                ".claude-plugin/marketplace.json",
                "teamwire/.claude-plugin/plugin.json",
                "teamwire/.mcp.json",
                "teamwire/hooks/hooks.json",
                "teamwire/skills/teamwire/SKILL.md",
                "teamwire/skills/teamwire/reference.md",
                "teamwire/skills/afk/SKILL.md",
                "teamwire/skills/promptwire/SKILL.md",
            ]
        );
        let market = json_of(&files, ".claude-plugin/marketplace.json");
        assert_eq!(market["name"], "teamwire");
        assert_eq!(market["plugins"][0]["name"], "teamwire");
        assert_eq!(market["plugins"][0]["source"], "./teamwire");
        assert_eq!(
            json_of(&files, "teamwire/.mcp.json"),
            json!({"mcpServers": {"teamwire": {
                "command": "/opt/bin/teamwire", "args": ["mcp", "--lead"]}}})
        );
        assert_eq!(
            json_of(&files, "teamwire/hooks/hooks.json"),
            json!({"hooks": {"Stop": [{"hooks": [{
                "type": "command",
                "command": "/opt/bin/teamwire hook claude-lead-stop",
                "timeout": 10}]}]}})
        );
        assert!(file(&files, "teamwire/skills/teamwire/SKILL.md").contains("teamwire-sha256: "));
        let afk = file(&files, "teamwire/skills/afk/SKILL.md");
        assert!(afk.contains("\nname: afk\n") && afk.contains("teamwire-sha256: "));
        let promptwire = file(&files, "teamwire/skills/promptwire/SKILL.md");
        assert!(
            promptwire.contains("\nname: promptwire\n") && promptwire.contains("teamwire-sha256: ")
        );
    }

    #[test]
    fn the_reference_file_is_the_compiled_in_text_and_changes_the_version() {
        let version = |reference: &str| {
            let files = PluginFiles::render_with(Path::new("/x"), &want(None), reference).unwrap();
            let manifest = json_of(&files, "teamwire/.claude-plugin/plugin.json");
            let text = file(&files, "teamwire/skills/teamwire/reference.md").to_owned();
            (manifest["version"].as_str().unwrap().to_owned(), text)
        };
        let (a, text) = version(REFERENCE_TEXT);
        assert_eq!(text, REFERENCE_TEXT);
        assert!(text.contains("## Watch lines"));
        let (b, _) = version(&format!("{REFERENCE_TEXT}x"));
        assert_ne!(a, b);
    }

    #[test]
    fn a_home_that_is_not_the_default_reaches_the_server_and_the_hook() {
        let files = PluginFiles::render(Path::new("/d"), &want(Some("/scratch tw"))).unwrap();
        assert_eq!(
            json_of(&files, "teamwire/.mcp.json")["mcpServers"]["teamwire"]["args"],
            json!(["--home", "/scratch tw", "mcp", "--lead"])
        );
        // The hook command runs through a shell, so the space is quoted.
        assert_eq!(
            json_of(&files, "teamwire/hooks/hooks.json")["hooks"]["Stop"][0]["hooks"][0]["command"],
            "/opt/bin/teamwire --home '/scratch tw' hook claude-lead-stop"
        );
    }

    #[test]
    fn the_version_changes_with_the_binary_and_not_with_the_directory() {
        let version = |dir: &str, e: &McpEntry| {
            json_of(
                &PluginFiles::render(Path::new(dir), e).unwrap(),
                "teamwire/.claude-plugin/plugin.json",
            )["version"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let a = version("/x", &want(None));
        assert_eq!(a.len(), 12);
        assert_eq!(a, version("/y", &want(None)));
        let other = McpEntry::lead(Path::new("/new/teamwire"), None);
        assert_ne!(a, version("/x", &other));
    }

    #[test]
    fn written_and_loaded_copies_are_compared_by_text() {
        let tmp = tempfile::tempdir().unwrap();
        let files = PluginFiles::render(tmp.path(), &want(None)).unwrap();
        assert!(!files.is_written());
        for (path, text) in &files.files {
            let to = tmp.path().join(path);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::write(to, text).unwrap();
        }
        assert!(files.is_written());
        assert!(files.plugin_matches(&tmp.path().join("teamwire")));
        std::fs::write(tmp.path().join("teamwire/.mcp.json"), "{}").unwrap();
        assert!(!files.is_written());
        assert!(!files.plugin_matches(&tmp.path().join("teamwire")));
    }

    #[test]
    fn commands_name_the_plugin_and_user_scope() {
        let config = ClaudeConfig {
            claude_dir: PathBuf::from("/u/.claude"),
            claude_json: PathBuf::from("/u/.claude.json"),
            claude_bin: "claude".to_owned(),
        };
        assert_eq!(
            plugin_verb(&config, "install").args,
            ["plugin", "install", "teamwire@teamwire", "--scope", "user"]
        );
        assert_eq!(
            marketplace_add(&config, Path::new("/h/claude-plugin")).args,
            [
                "plugin",
                "marketplace",
                "add",
                "--scope",
                "user",
                "/h/claude-plugin"
            ]
        );
        assert!(marketplace_update(&config).env.is_empty());
    }
}
