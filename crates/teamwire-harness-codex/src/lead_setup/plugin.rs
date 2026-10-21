//! The teamwire plugin for Codex (ADR 0023 point 9): the files `teamwire
//! install` renders under `$TEAMWIRE_HOME/codex-plugin/`. Codex keeps its own
//! copy of an installed plugin, so a changed plugin has a new `version`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use teamwire_core::LaunchSpec;

/// The marketplace and the plugin in it, both named `teamwire`.
pub(crate) const MARKETPLACE: &str = "teamwire";
pub(crate) const PLUGIN_ID: &str = "teamwire@teamwire";

/// The hook timeout in seconds. The hook only reads local files.
const STOP_HOOK_TIMEOUT_S: u64 = 10;

/// The plugin's directory inside the marketplace.
const PLUGIN_DIR: &str = "teamwire";

/// How a Codex lead differs from the Claude Code lead the shared skill
/// describes. It goes between the frontmatter and the shared text.
const CODEX_PART: &str = include_str!("../../../../examples/skills/codex-lead.md");

/// The sections `SKILL.md` leaves to a second file. They need no Codex part,
/// so the plugin carries the text as it is.
const REFERENCE_TEXT: &str = include_str!("../../../../examples/skills/teamwire/reference.md");

/// The skill a lead loads when promptwire is installed. It names no Claude
/// tool, so the plugin carries it as it is.
const PROMPTWIRE_TEXT: &str = include_str!("../../../../examples/skills/promptwire/SKILL.md");

const STAMP_KEY: &str = "  teamwire-sha256: ";

/// The program the plugin runs: `<bin> [--home DIR]`, then `mcp --lead` or
/// the hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadCommand {
    pub bin: PathBuf,
    /// Given only when the state directory is not the default, so a default
    /// install keeps following the default.
    pub home: Option<PathBuf>,
}

impl LeadCommand {
    fn words(&self) -> Vec<String> {
        let mut words = vec![self.bin.display().to_string()];
        if let Some(home) = &self.home {
            // Rebuilt from its components so `/x/` and `/x` give one command.
            let home: PathBuf = home.components().collect();
            words.extend(["--home".to_owned(), home.display().to_string()]);
        }
        words
    }
}

/// The marketplace directory's files, as this binary renders them: each path
/// is relative to the directory, with its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginFiles {
    pub(crate) dir: PathBuf,
    pub(crate) files: Vec<(PathBuf, String)>,
    /// The plugin's `version`: a hash of its files.
    pub(crate) version: String,
}

/// `<home>/codex-plugin`: the marketplace directory.
pub(crate) fn plugin_dir(home: &Path) -> PathBuf {
    home.join("codex-plugin")
}

fn pretty(doc: &Value) -> String {
    // A `json!` value always serializes; the fallback keeps the function total.
    serde_json::to_string_pretty(doc).map_or_else(|_| doc.to_string(), |t| t + "\n")
}

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The skill: the shared text's frontmatter, the Codex part, the shared body,
/// and a `metadata` block holding the hash of that text as the last
/// frontmatter key. `None` when the shared text has no frontmatter.
fn skill(shared: &str) -> Option<String> {
    let rest = shared.strip_prefix("---\n")?;
    // `fence` starts the closing `---` line; the Codex part goes after it.
    let fence = "---\n".len() + rest.find("\n---\n")? + 1;
    let (head, body) = shared.split_at(fence + "---\n".len());
    let text = format!("{head}\n{CODEX_PART}{body}");
    let (head, tail) = text.split_at(fence);
    Some(format!(
        "{head}metadata:\n{STAMP_KEY}{}\n{tail}",
        sha256_hex(&text)
    ))
}

impl PluginFiles {
    /// The files for the marketplace at `dir`, running `want`. Fails when the
    /// shared skill has no frontmatter.
    pub(crate) fn render(dir: &Path, want: &LeadCommand, shared: &str) -> Result<Self, String> {
        Self::render_with(dir, want, shared, REFERENCE_TEXT)
    }

    /// `render` with the text of `reference.md` given, so a test can change it
    /// alone.
    fn render_with(
        dir: &Path,
        want: &LeadCommand,
        shared: &str,
        reference: &str,
    ) -> Result<Self, String> {
        let skill =
            skill(shared).ok_or_else(|| "the compiled-in skill has no frontmatter".to_owned())?;
        let mut mcp_args = want.words().split_off(1);
        mcp_args.extend(["mcp".to_owned(), "--lead".to_owned()]);
        let mut hook_words: Vec<String> = want
            .words()
            .iter()
            .map(|w| LaunchSpec::shell_word(w))
            .collect();
        hook_words.extend(["hook", "lead-stop", "--harness", "codex"].map(str::to_owned));
        let body = [
            (
                format!("{PLUGIN_DIR}/.mcp.json"),
                pretty(&json!({"mcpServers": {"teamwire": {
                    "command": want.bin.display().to_string(),
                    "args": mcp_args,
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
            (
                format!("{PLUGIN_DIR}/skills/promptwire/SKILL.md"),
                PROMPTWIRE_TEXT.to_owned(),
            ),
        ];
        let mut content = String::new();
        for (path, text) in &body {
            content.push_str(path);
            content.push('\0');
            content.push_str(text);
            content.push('\0');
        }
        let version: String = sha256_hex(&content).chars().take(12).collect();
        let manifest = (
            format!("{PLUGIN_DIR}/.codex-plugin/plugin.json"),
            pretty(&json!({
                "name": MARKETPLACE,
                "version": version,
                "description": "Lead tools, skill and Stop hook for teamwire teams",
            })),
        );
        let marketplace = (
            ".agents/plugins/marketplace.json".to_owned(),
            pretty(&json!({
                "name": MARKETPLACE,
                "plugins": [{
                    "name": MARKETPLACE,
                    "source": {"source": "local", "path": format!("./{PLUGIN_DIR}")},
                    "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
                    "category": "Productivity",
                }],
            })),
        );
        Ok(Self {
            dir: dir.to_path_buf(),
            version,
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
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    reason = "the tests index JSON they just built and write the plugin files as install would"
)]
mod tests {
    use super::*;

    const SHARED: &str = "---\nname: teamwire\ndescription: d\n---\n\n# Body\n";

    fn want(home: Option<&str>) -> LeadCommand {
        LeadCommand {
            bin: PathBuf::from("/opt/bin/teamwire"),
            home: home.map(PathBuf::from),
        }
    }

    fn file<'a>(files: &'a PluginFiles, path: &str) -> &'a str {
        files
            .files
            .iter()
            .find(|(p, _)| p == Path::new(path))
            .map(|(_, text)| text.as_str())
            .unwrap_or_else(|| unreachable!("no file {path}"))
    }

    #[test]
    fn the_files_name_the_binary_and_the_codex_hook() {
        let files = PluginFiles::render(Path::new("/h/codex-plugin"), &want(None), SHARED).unwrap();
        let paths: Vec<_> = files
            .files
            .iter()
            .map(|(p, _)| p.display().to_string())
            .collect();
        assert_eq!(
            paths,
            [
                ".agents/plugins/marketplace.json",
                "teamwire/.codex-plugin/plugin.json",
                "teamwire/.mcp.json",
                "teamwire/hooks/hooks.json",
                "teamwire/skills/teamwire/SKILL.md",
                "teamwire/skills/teamwire/reference.md",
                "teamwire/skills/promptwire/SKILL.md",
            ]
        );
        assert_eq!(
            file(&files, "teamwire/skills/promptwire/SKILL.md"),
            PROMPTWIRE_TEXT
        );
        assert_eq!(
            file(&files, "teamwire/.mcp.json"),
            r#"{
  "mcpServers": {
    "teamwire": {
      "args": [
        "mcp",
        "--lead"
      ],
      "command": "/opt/bin/teamwire"
    }
  }
}
"#
        );
        assert_eq!(
            file(&files, "teamwire/hooks/hooks.json"),
            r#"{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "command": "/opt/bin/teamwire hook lead-stop --harness codex",
            "timeout": 10,
            "type": "command"
          }
        ]
      }
    ]
  }
}
"#
        );
    }

    #[test]
    fn a_home_that_is_not_the_default_reaches_the_server_and_the_hook() {
        let files =
            PluginFiles::render(Path::new("/d"), &want(Some("/scratch tw/")), SHARED).unwrap();
        let mcp: Value = serde_json::from_str(file(&files, "teamwire/.mcp.json")).unwrap();
        assert_eq!(
            mcp["mcpServers"]["teamwire"]["args"],
            json!(["--home", "/scratch tw", "mcp", "--lead"])
        );
        let hooks: Value = serde_json::from_str(file(&files, "teamwire/hooks/hooks.json")).unwrap();
        assert_eq!(
            hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
            "/opt/bin/teamwire --home '/scratch tw' hook lead-stop --harness codex"
        );
    }

    #[test]
    fn the_skill_is_the_codex_part_then_the_shared_text_with_a_stamp() {
        let files = PluginFiles::render(Path::new("/d"), &want(None), SHARED).unwrap();
        let text = file(&files, "teamwire/skills/teamwire/SKILL.md");
        assert!(
            text.starts_with("---\nname: teamwire\ndescription: d\nmetadata:\n  teamwire-sha256: ")
        );
        let codex = text.find("codex --no-daemon").unwrap();
        assert!(codex < text.find("# Body").unwrap());
    }

    #[test]
    fn the_codex_part_of_the_real_skill_sits_in_the_body() {
        let shared = include_str!("../../../../examples/skills/teamwire/SKILL.md");
        let files = PluginFiles::render(Path::new("/d"), &want(None), shared).unwrap();
        let text = file(&files, "teamwire/skills/teamwire/SKILL.md");
        let rest = text.strip_prefix("---\n").unwrap();
        let end = rest.find("\n---\n").unwrap();
        let (frontmatter, body) = rest.split_at(end);
        assert!(!frontmatter.contains("codex --no-daemon"), "{frontmatter}");
        assert!(frontmatter.contains(STAMP_KEY), "{frontmatter}");
        assert_eq!(body.matches("codex --no-daemon").count(), 1, "{body}");
    }

    #[test]
    fn the_reference_file_is_the_compiled_in_text_and_changes_the_version() {
        let render = |reference: &str| {
            PluginFiles::render_with(Path::new("/x"), &want(None), SHARED, reference).unwrap()
        };
        let a = render(REFERENCE_TEXT);
        assert_eq!(
            file(&a, "teamwire/skills/teamwire/reference.md"),
            REFERENCE_TEXT
        );
        assert_ne!(a.version, render(&format!("{REFERENCE_TEXT}x")).version);
    }

    #[test]
    fn the_version_and_stamp_change_with_the_text_and_not_with_the_directory() {
        let render = |dir: &str, c: &LeadCommand, shared: &str| {
            PluginFiles::render(Path::new(dir), c, shared).unwrap()
        };
        let a = render("/x", &want(None), SHARED);
        assert_eq!(a.version.len(), 12);
        assert_eq!(a.version, render("/y", &want(None), SHARED).version);
        let other = LeadCommand {
            bin: PathBuf::from("/new/teamwire"),
            home: None,
        };
        assert_ne!(a.version, render("/x", &other, SHARED).version);
        let edited = render("/x", &want(None), &SHARED.replace("# Body", "# Body 2"));
        assert_ne!(a.version, edited.version);
        let stamp = |f: &PluginFiles| {
            file(f, "teamwire/skills/teamwire/SKILL.md")
                .lines()
                .find(|l| l.starts_with(STAMP_KEY))
                .map(str::to_owned)
        };
        assert_ne!(stamp(&a), stamp(&edited));
        assert!(PluginFiles::render(Path::new("/x"), &want(None), "no frontmatter").is_err());
    }

    #[test]
    fn written_files_are_compared_by_text() {
        let tmp = tempfile::tempdir().unwrap();
        let files = PluginFiles::render(tmp.path(), &want(None), SHARED).unwrap();
        assert!(!files.is_written());
        for (path, text) in &files.files {
            let to = tmp.path().join(path);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::write(to, text).unwrap();
        }
        assert!(files.is_written());
        std::fs::write(tmp.path().join("teamwire/.mcp.json"), "{}").unwrap();
        assert!(!files.is_written());
    }
}
