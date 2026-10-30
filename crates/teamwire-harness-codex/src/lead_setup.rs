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

pub use teamwire_core::{LeadCommand, LeadSetupError, LeadStatus};

use std::fmt;
use std::path::{Path, PathBuf};

use teamwire_core::lead_setup::{DoctorCheck, apply_in_order, make_step, optional_check};
use teamwire_core::{ApplyError, LeadChange, LeadPlan, LeadSetup, LeadStep, LeadTarget};

use plugin::{MARKETPLACE, PLUGIN_ID, PluginFiles, plugin_dir};
use state::MarketplaceState;

use crate::CodexCli;

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

    /// What making the change does.
    fn step(&self, cli: &CodexCli) -> LeadStep {
        let plugin = |args: &[&str]| LeadStep::Run(cli.plugin_command(args));
        match self {
            Self::WritePlugin { dir, files } => LeadStep::WriteFiles {
                dir: dir.clone(),
                files: files.clone(),
            },
            Self::RemovePluginFiles(dir) => LeadStep::RemoveDir(dir.clone()),
            Self::AddMarketplace(dir) => {
                plugin(&["marketplace", "add", &dir.display().to_string()])
            }
            Self::RemoveMarketplace => plugin(&["marketplace", "remove", MARKETPLACE]),
            Self::AddPlugin => plugin(&["add", PLUGIN_ID]),
            Self::RemovePlugin => plugin(&["remove", PLUGIN_ID]),
        }
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

const FOREIGN_MARKETPLACE: &str =
    "a Codex marketplace named `teamwire` is registered from another source";

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
    ) -> Result<LeadStatus, LeadSetupError> {
        let dir = plugin_dir(home);
        let files =
            PluginFiles::render(&dir, want, shared_skill).map_err(LeadSetupError::Refused)?;
        let state = self.plugin_state(&dir)?;
        let Some(installed) = state.installed else {
            return Ok(LeadStatus::Missing);
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
            LeadStatus::Current
        } else {
            LeadStatus::Stale(problems)
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
                return Err(LeadSetupError::needs_force(FOREIGN_MARKETPLACE, "replace"));
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
                return Err(LeadSetupError::needs_force(FOREIGN_MARKETPLACE, "remove"));
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
    pub fn apply_lead_setup(&self, changes: &[Change]) -> Result<(), Box<ApplyError<Change>>> {
        apply_in_order(changes, |c| {
            make_step(self.platform.as_ref(), &c.step(self))
        })
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
}

impl LeadSetup for CodexCli {
    fn program(&self) -> &str {
        &self.config.codex_bin
    }

    fn label(&self) -> &'static str {
        "Codex"
    }

    fn status(&self, target: &LeadTarget<'_>) -> Result<LeadStatus, LeadSetupError> {
        self.plugin_status(
            target.command,
            target.shared_skill,
            target.home,
            target.compare_files,
        )
    }

    fn plan_install(
        &self,
        target: &LeadTarget<'_>,
        force: bool,
    ) -> Result<LeadPlan, LeadSetupError> {
        let changes = Self::plan_install(
            self,
            target.command,
            target.shared_skill,
            target.home,
            force,
        )?;
        Ok(self.plan(&changes))
    }

    fn plan_uninstall(&self, home: &Path, force: bool) -> Result<LeadPlan, LeadSetupError> {
        let changes = Self::plan_uninstall(self, home, force)?;
        Ok(self.plan(&changes))
    }

    fn apply(&self, changes: &[LeadChange]) -> Result<(), Box<ApplyError>> {
        apply_in_order(changes, |c| make_step(self.platform.as_ref(), c.step()))
    }

    /// A missing plugin is fine: Codex is optional. A stale one warns.
    fn doctor_checks(
        &self,
        target: Result<&LeadTarget<'_>, &str>,
        _dir: &Path,
    ) -> Vec<DoctorCheck> {
        let Ok(target) = target else {
            return Vec::new();
        };
        vec![(
            "codex plugin",
            optional_check(self.status(target), Self::ID, self.label()),
        )]
    }
}

impl CodexCli {
    fn plan(&self, changes: &[Change]) -> LeadPlan {
        let note = changes
            .iter()
            .any(Change::needs_hook_review)
            .then_some("Codex will ask you to review the plugin's hook once after this change");
        LeadPlan {
            changes: changes
                .iter()
                .map(|c| LeadChange::new(c.to_string(), c.step(self)))
                .collect(),
            note,
        }
    }
}

#[cfg(test)]
mod tests;
