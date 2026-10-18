//! The lead setup for Codex: the `teamwire` plugin (ADR 0023 point 9), added
//! and removed only through `codex plugin`. teamwire never writes
//! `$CODEX_HOME`; it reads `config.toml` and `codex plugin list --json`.
//!
//! Install and uninstall run in two halves. `plan_*` reads Codex's setup,
//! runs every check, and returns the changes; nothing is written. `apply`
//! makes them in order and stops at the first failure. A caller that only
//! plans has a dry run.

mod plugin;
mod state;

pub use plugin::LeadCommand;

use std::fmt;
use std::path::{Path, PathBuf};

use plugin::{MARKETPLACE, PLUGIN_ID, PluginFiles, plugin_dir};
use state::MarketplaceState;

use crate::CodexCli;

#[derive(Debug, thiserror::Error)]
pub enum LeadSetupError {
    #[error("cannot read {}: {detail}", path.display())]
    Read { path: PathBuf, detail: String },
    /// A check failed before anything was written.
    #[error("{0}")]
    Refused(String),
}

/// One change install or uninstall makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Write the plugin's files into the marketplace directory: each path is
    /// relative to `dir`.
    WritePlugin {
        dir: PathBuf,
        files: Vec<(PathBuf, String)>,
    },
    AddMarketplace(PathBuf),
    RemoveMarketplace,
    AddPlugin,
    RemovePlugin,
    /// Delete the marketplace directory.
    RemovePluginFiles(PathBuf),
}

impl Change {
    /// True for a change made by running `codex`.
    pub fn is_codex(&self) -> bool {
        !matches!(self, Self::WritePlugin { .. } | Self::RemovePluginFiles(_))
    }

    /// True when Codex asks the user to review the plugin's hook afterwards.
    pub fn needs_hook_review(&self) -> bool {
        matches!(self, Self::AddPlugin)
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WritePlugin { dir, .. } => {
                write!(f, "write the teamwire plugin files to {}", dir.display())
            }
            Self::AddMarketplace(dir) => write!(
                f,
                "add the Codex marketplace `{MARKETPLACE}` from {}",
                dir.display()
            ),
            Self::RemoveMarketplace => write!(f, "remove the Codex marketplace `{MARKETPLACE}`"),
            Self::AddPlugin => write!(f, "add the Codex plugin `{PLUGIN_ID}`"),
            Self::RemovePlugin => write!(f, "remove the Codex plugin `{PLUGIN_ID}`"),
            Self::RemovePluginFiles(dir) => {
                write!(f, "delete the teamwire plugin files at {}", dir.display())
            }
        }
    }
}

/// A change that failed after the checks passed.
#[derive(Debug, thiserror::Error)]
#[error("{failed} failed: {detail}")]
pub struct ApplyError {
    /// The changes made before the failure, in order.
    pub done: Vec<Change>,
    pub failed: Change,
    pub detail: String,
}

/// What Codex has of the plugin, compared with what this binary would install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    /// The plugin is not installed.
    Missing,
    /// Installed, but another version, disabled, or from another source.
    Stale(Vec<String>),
    Current,
}

const FOREIGN_MARKETPLACE: &str =
    "a Codex marketplace named `teamwire` is registered from another source";

fn refused(what: &str, verb: &str) -> LeadSetupError {
    LeadSetupError::Refused(format!("{what}; pass --force to {verb} it"))
}

impl CodexCli {
    /// What Codex has of the plugin. `compare_files` is false for a cargo
    /// build, whose text differs from the installed binary's by design.
    /// Reads only, so `doctor` calls it.
    pub fn plugin_status(
        &self,
        want: &LeadCommand,
        shared_skill: &str,
        home: &Path,
        compare_files: bool,
    ) -> Result<PluginStatus, LeadSetupError> {
        let dir = plugin_dir(home);
        let files =
            PluginFiles::render(&dir, want, shared_skill).map_err(LeadSetupError::Refused)?;
        let state = self.plugin_state(&dir)?;
        let Some(installed) = state.installed else {
            return Ok(PluginStatus::Missing);
        };
        let mut problems = Vec::new();
        if compare_files && installed.version.as_deref() != Some(files.version.as_str()) {
            problems.push(format!(
                "the installed plugin is version {}, not this binary's {}",
                installed.version.as_deref().unwrap_or("unknown"),
                files.version
            ));
        }
        if !installed.enabled {
            problems.push(format!("`{PLUGIN_ID}` is disabled"));
        }
        if state.marketplace == MarketplaceState::Foreign {
            problems.push(FOREIGN_MARKETPLACE.to_owned());
        }
        Ok(if problems.is_empty() {
            PluginStatus::Current
        } else {
            PluginStatus::Stale(problems)
        })
    }

    /// The changes that make the plugin this binary's, after every check.
    /// `home` is the state directory, whose `codex-plugin` holds the files.
    /// `force` replaces a marketplace from another source.
    pub fn plan_install(
        &self,
        want: &LeadCommand,
        shared_skill: &str,
        home: &Path,
        force: bool,
    ) -> Result<Vec<Change>, LeadSetupError> {
        let dir = plugin_dir(home);
        let files =
            PluginFiles::render(&dir, want, shared_skill).map_err(LeadSetupError::Refused)?;
        let state = self.plugin_state(&dir)?;
        let replace = match state.marketplace {
            MarketplaceState::Foreign if !force => {
                return Err(refused(FOREIGN_MARKETPLACE, "replace"));
            }
            MarketplaceState::Foreign => true,
            MarketplaceState::Missing | MarketplaceState::Ours => false,
        };
        // Codex copies a plugin when it is added, so a changed plugin is
        // removed and added again.
        let reinstall = state.installed.as_ref().is_some_and(|i| {
            !i.enabled || i.version.as_deref() != Some(files.version.as_str()) || replace
        });
        let mut changes = Vec::new();
        if !files.is_written() {
            changes.push(Change::WritePlugin {
                dir: files.dir,
                files: files.files,
            });
        }
        if reinstall {
            changes.push(Change::RemovePlugin);
        }
        if replace {
            changes.push(Change::RemoveMarketplace);
        }
        if state.marketplace == MarketplaceState::Missing || replace {
            changes.push(Change::AddMarketplace(dir));
        }
        if state.installed.is_none() || reinstall {
            changes.push(Change::AddPlugin);
        }
        self.check_codex_runs(&changes)?;
        Ok(changes)
    }

    /// The changes that remove the plugin, after every check. `force` also
    /// removes a marketplace from another source.
    pub fn plan_uninstall(&self, home: &Path, force: bool) -> Result<Vec<Change>, LeadSetupError> {
        let dir = plugin_dir(home);
        let state = self.plugin_state(&dir)?;
        let mut changes = Vec::new();
        if state.installed.is_some() {
            changes.push(Change::RemovePlugin);
        }
        match state.marketplace {
            MarketplaceState::Missing => {}
            MarketplaceState::Foreign if !force => {
                return Err(refused(FOREIGN_MARKETPLACE, "remove"));
            }
            MarketplaceState::Ours | MarketplaceState::Foreign => {
                changes.push(Change::RemoveMarketplace);
            }
        }
        if dir.exists() {
            changes.push(Change::RemovePluginFiles(dir));
        }
        self.check_codex_runs(&changes)?;
        Ok(changes)
    }

    /// Makes `changes` in order, stopping at the first failure.
    pub fn apply_lead_setup(&self, changes: &[Change]) -> Result<(), Box<ApplyError>> {
        let mut done = Vec::new();
        for change in changes {
            if let Err(detail) = self.apply_one(change) {
                return Err(Box::new(ApplyError {
                    done,
                    failed: change.clone(),
                    detail,
                }));
            }
            done.push(change.clone());
        }
        Ok(())
    }

    fn apply_one(&self, change: &Change) -> Result<(), String> {
        let plugin = |args: &[&str]| self.run_codex(&self.plugin_command(args)).map(drop);
        match change {
            Change::WritePlugin { dir, files } => self.write_plugin(dir, files),
            Change::RemovePluginFiles(dir) => std::fs::remove_dir_all(dir)
                .map_err(|e| format!("cannot remove {}: {e}", dir.display())),
            Change::AddMarketplace(dir) => {
                plugin(&["marketplace", "add", &dir.display().to_string()])
            }
            Change::RemoveMarketplace => plugin(&["marketplace", "remove", MARKETPLACE]),
            Change::AddPlugin => plugin(&["add", PLUGIN_ID]),
            Change::RemovePlugin => plugin(&["remove", PLUGIN_ID]),
        }
    }

    /// Runs `codex plugin --help` when a change needs `codex`, so a `codex`
    /// without the `plugin` command is refused before the first write.
    fn check_codex_runs(&self, changes: &[Change]) -> Result<(), LeadSetupError> {
        if !changes.iter().any(Change::is_codex) {
            return Ok(());
        }
        self.run_codex(&self.plugin_command(&["--help"]))
            .map(drop)
            .map_err(|detail| {
                LeadSetupError::Refused(format!(
                    "cannot run `{} plugin`, which registers the teamwire plugin: {detail}",
                    self.config.codex_bin
                ))
            })
    }

    fn write_plugin(&self, dir: &Path, files: &[(PathBuf, String)]) -> Result<(), String> {
        for (rel, text) in files {
            let path = dir.join(rel);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            }
            self.platform
                .write_atomic(&path, text.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
