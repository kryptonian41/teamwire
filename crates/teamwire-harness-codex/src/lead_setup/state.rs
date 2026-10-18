//! What Codex has registered from the teamwire plugin: the marketplace from
//! `$CODEX_HOME/config.toml` and the installed plugin from `codex plugin
//! list --json`. Both are only read; every write goes through `codex plugin`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use teamwire_core::Command;

use super::LeadSetupError;
use super::plugin::{MARKETPLACE, PLUGIN_ID};
use crate::CodexCli;

/// How long one `codex` command may take.
pub(crate) const CODEX_TIMEOUT: Duration = Duration::from_secs(30);

/// Where the user's `teamwire` marketplace came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarketplaceState {
    Missing,
    Ours,
    /// Registered from some other source.
    Foreign,
}

/// The installed plugin, as `codex plugin list --json` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) version: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginState {
    pub(crate) marketplace: MarketplaceState,
    pub(crate) installed: Option<Installed>,
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

fn marketplace_state(config_toml: &Path, dir: &Path) -> Result<MarketplaceState, LeadSetupError> {
    let read_err = |detail: String| LeadSetupError::Read {
        path: config_toml.to_path_buf(),
        detail,
    };
    let text = match std::fs::read_to_string(config_toml) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(MarketplaceState::Missing),
        Err(e) => return Err(read_err(e.to_string())),
    };
    let doc: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| read_err(e.to_string()))?;
    let Some(entry) = doc.get("marketplaces").and_then(|m| m.get(MARKETPLACE)) else {
        return Ok(MarketplaceState::Missing);
    };
    let text = |k: &str| entry.get(k).and_then(toml::Value::as_str);
    Ok(match (text("source_type"), text("source")) {
        (Some("local"), Some(from)) if same_dir(Path::new(from), dir) => MarketplaceState::Ours,
        _ => MarketplaceState::Foreign,
    })
}

impl CodexCli {
    pub(crate) fn codex(&self) -> Command {
        Command::new(self.config.codex_bin.clone())
            .env("CODEX_HOME", self.config.codex_home.display().to_string())
    }

    /// `codex plugin <args>`.
    pub(crate) fn plugin_command(&self, args: &[&str]) -> Command {
        self.codex()
            .arg("plugin")
            .args(args.iter().map(|a| (*a).to_owned()))
    }

    /// Runs `cmd` and returns its stdout, or what went wrong in words.
    pub(crate) fn run_codex(&self, cmd: &Command) -> Result<String, String> {
        let out = self
            .platform
            .run(cmd, CODEX_TIMEOUT)
            .map_err(|e| e.to_string())?;
        if out.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let said = if stderr.is_empty() { stdout } else { stderr };
        Err(if out.timed_out {
            format!("timed out after {}s", CODEX_TIMEOUT.as_secs())
        } else {
            format!("exit {}: {said}", out.exit_code)
        })
    }

    fn installed(&self) -> Result<Option<Installed>, LeadSetupError> {
        let refused = |detail: String| {
            LeadSetupError::Refused(format!(
                "cannot read the installed Codex plugins with `{} plugin list --json`: {detail}",
                self.config.codex_bin
            ))
        };
        let text = self
            .run_codex(&self.plugin_command(&["list", "--json"]))
            .map_err(refused)?;
        let doc: Value = serde_json::from_str(&text).map_err(|e| refused(e.to_string()))?;
        Ok(doc
            .get("installed")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|p| p.get("pluginId").and_then(Value::as_str) == Some(PLUGIN_ID))
            .map(|p| Installed {
                version: p.get("version").and_then(Value::as_str).map(str::to_owned),
                enabled: p.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            }))
    }

    /// What Codex has registered, for the marketplace at `dir`. Reads only.
    pub(crate) fn plugin_state(&self, dir: &Path) -> Result<PluginState, LeadSetupError> {
        let config_toml: PathBuf = self.config.codex_home.join("config.toml");
        Ok(PluginState {
            marketplace: marketplace_state(&config_toml, dir)?,
            installed: self.installed()?,
        })
    }
}
