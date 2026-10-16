//! The lead setup: the `teamwire` plugin in Claude Code (ADR 0018), and the
//! removal of the skill and MCP entry that ADR 0012 wrote.
//!
//! Install and uninstall run in two halves. `plan_*` reads both setups, runs
//! every check, and returns the changes; nothing is written. `apply` makes
//! them in order and stops at the first failure. A caller that only plans
//! has a dry run.

mod mcp;
mod plugin;
mod shadow;
mod skill;

pub use mcp::{McpEntry, McpState};
pub use shadow::{Shadow, shadowing_entries};
pub use skill::{SKILL_TEXT, SkillState};

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use plugin::{MarketplaceState, PLUGIN_ID, PluginFiles, plugin_dir, plugin_state};

use crate::{ClaudeCode, ClaudeConfig};

/// How long one `claude` command may take.
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum LeadSetupError {
    #[error("cannot read {}: {detail}", path.display())]
    Read { path: PathBuf, detail: String },
    /// A check failed before anything was written.
    #[error("{0}")]
    Refused(String),
}

/// The skill file and MCP entry that ADR 0012 wrote, as they are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadSetupState {
    pub skill_path: PathBuf,
    pub skill: SkillState,
    pub mcp: McpState,
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
    UpdateMarketplace,
    RemoveMarketplace,
    InstallPlugin,
    /// Claude Code keeps its own copy of a plugin, so a new binary needs this.
    UpdatePlugin,
    EnablePlugin,
    UninstallPlugin,
    /// Delete the marketplace directory.
    RemovePluginFiles(PathBuf),
    /// The skill file ADR 0012 wrote.
    RemoveSkill(PathBuf),
    /// The user MCP entry ADR 0012 wrote, described.
    RemoveMcp(String),
}

impl Change {
    /// True for a change made by running `claude`.
    pub fn is_claude(&self) -> bool {
        !matches!(
            self,
            Self::WritePlugin { .. } | Self::RemovePluginFiles(_) | Self::RemoveSkill(_)
        )
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WritePlugin { dir, .. } => {
                write!(f, "write the teamwire plugin files to {}", dir.display())
            }
            Self::AddMarketplace(dir) => {
                write!(
                    f,
                    "add the Claude Code marketplace `teamwire` from {}",
                    dir.display()
                )
            }
            Self::UpdateMarketplace => write!(f, "update the Claude Code marketplace `teamwire`"),
            Self::RemoveMarketplace => write!(f, "remove the Claude Code marketplace `teamwire`"),
            Self::InstallPlugin => write!(f, "install the plugin `{PLUGIN_ID}` at user scope"),
            Self::UpdatePlugin => write!(f, "update the plugin `{PLUGIN_ID}` at user scope"),
            Self::EnablePlugin => write!(f, "enable the plugin `{PLUGIN_ID}` at user scope"),
            Self::UninstallPlugin => write!(f, "uninstall the plugin `{PLUGIN_ID}` at user scope"),
            Self::RemovePluginFiles(dir) => {
                write!(f, "delete the teamwire plugin files at {}", dir.display())
            }
            Self::RemoveSkill(p) => write!(f, "remove the old teamwire skill at {}", p.display()),
            Self::RemoveMcp(old) => {
                write!(f, "remove the old user MCP server `teamwire` ({old})")
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

/// Why an item was refused, with the way past it: `--force` to `verb` it.
fn refused(what: &str, verb: &str) -> LeadSetupError {
    LeadSetupError::Refused(format!("{what}; pass --force to {verb} it"))
}

/// The refusal text for each kind of item that is not teamwire's.
pub fn edited_skill(path: &Path) -> String {
    format!(
        "{} was edited after teamwire wrote it, or is a copy installed by hand \
         from another version",
        path.display()
    )
}

fn foreign_entry(found: &str) -> String {
    format!("the user MCP server `teamwire` is not teamwire's lead server: {found}")
}

const FOREIGN_MARKETPLACE: &str =
    "a Claude Code marketplace named `teamwire` is registered from another source";

/// The old skill file's path and state. Reads only.
pub fn skill_state(config: &ClaudeConfig) -> Result<(PathBuf, SkillState), LeadSetupError> {
    let path = skill::path(&config.claude_dir);
    let state = skill::read(&path, SKILL_TEXT)?;
    Ok((path, state))
}

/// The user-scope MCP entry compared with `want`. Reads only.
pub fn mcp_state(config: &ClaudeConfig, want: &McpEntry) -> Result<McpState, LeadSetupError> {
    mcp::read(&config.claude_json, want)
}

/// Reads both old items, comparing the MCP entry with `want`. Reads only, so
/// `doctor` calls it without a platform.
pub fn lead_setup_state(
    config: &ClaudeConfig,
    want: &McpEntry,
) -> Result<LeadSetupState, LeadSetupError> {
    let (skill_path, skill) = skill_state(config)?;
    Ok(LeadSetupState {
        skill_path,
        skill,
        mcp: mcp_state(config, want)?,
    })
}

impl ClaudeConfig {
    /// What is wrong with Claude Code's registration of the teamwire plugin,
    /// in words; empty when it is installed, enabled, and (when
    /// `compare_files`) holds the files `want` would render. Reads only.
    pub fn plugin_problems(
        &self,
        want: &McpEntry,
        compare_files: bool,
    ) -> Result<Vec<String>, LeadSetupError> {
        let files = PluginFiles::render(Path::new(""), want).map_err(LeadSetupError::Refused)?;
        let state = plugin_state(self, Path::new(""))?;
        let mut problems = Vec::new();
        match &state.installed {
            None => problems.push(format!("`{PLUGIN_ID}` is not installed at user scope")),
            Some(root) if compare_files && !files.plugin_matches(root) => problems.push(format!(
                "the installed plugin at {} is another version of teamwire's",
                root.display()
            )),
            Some(_) => {}
        }
        if state.installed.is_some() && state.enabled == Some(false) {
            problems.push(format!("`{PLUGIN_ID}` is disabled at user scope"));
        }
        Ok(problems)
    }
}

/// The changes that remove what ADR 0012 wrote. `force` also removes an
/// edited skill and a foreign entry.
fn plan_migration(config: &ClaudeConfig, force: bool) -> Result<Vec<Change>, LeadSetupError> {
    // An entry no one registered, so a teamwire entry reads as `Outdated` and
    // its text reaches the change list.
    let any = McpEntry::lead(Path::new("/"), None);
    let state = lead_setup_state(config, &any)?;
    let mut changes = Vec::new();
    match state.skill {
        SkillState::Missing => {}
        SkillState::Edited if !force => {
            return Err(refused(&edited_skill(&state.skill_path), "remove"));
        }
        SkillState::Current | SkillState::Outdated | SkillState::Edited => {
            changes.push(Change::RemoveSkill(state.skill_path.clone()));
        }
    }
    match &state.mcp {
        McpState::Missing => {}
        McpState::Foreign(found) if !force => {
            return Err(refused(&foreign_entry(found), "remove"));
        }
        McpState::Current | McpState::Outdated(_) | McpState::Foreign(_) => {
            changes.push(Change::RemoveMcp(describe(&state.mcp)));
        }
    }
    Ok(changes)
}

impl ClaudeCode {
    /// The changes that make the plugin this binary's and remove the ADR 0012
    /// setup, after every check. `home` is the state directory, whose
    /// `claude-plugin` holds the files. `force` replaces a marketplace from
    /// another source, and removes an edited old skill and a foreign entry.
    pub fn plan_install(
        &self,
        want: &McpEntry,
        home: &Path,
        force: bool,
    ) -> Result<Vec<Change>, LeadSetupError> {
        let dir = plugin_dir(home);
        let files = PluginFiles::render(&dir, want).map_err(LeadSetupError::Refused)?;
        let state = plugin_state(&self.config, &dir)?;
        let migration = plan_migration(&self.config, force)?;

        let written = files.is_written();
        let stale = state
            .installed
            .as_deref()
            .is_none_or(|root| !files.plugin_matches(root));
        let mut changes = Vec::new();
        if !written {
            changes.push(Change::WritePlugin {
                dir: files.dir,
                files: files.files,
            });
        }
        match state.marketplace {
            MarketplaceState::Missing => changes.push(Change::AddMarketplace(dir.clone())),
            MarketplaceState::Foreign if !force => {
                return Err(refused(FOREIGN_MARKETPLACE, "replace"));
            }
            MarketplaceState::Foreign => {
                changes.push(Change::RemoveMarketplace);
                changes.push(Change::AddMarketplace(dir.clone()));
            }
            MarketplaceState::Ours if !written || stale => {
                changes.push(Change::UpdateMarketplace);
            }
            MarketplaceState::Ours => {}
        }
        match &state.installed {
            None => changes.push(Change::InstallPlugin),
            Some(_) if stale => changes.push(Change::UpdatePlugin),
            Some(_) => {}
        }
        if state.installed.is_some() && state.enabled == Some(false) {
            changes.push(Change::EnablePlugin);
        }
        changes.extend(migration);
        self.check_claude_runs(&changes)?;
        Ok(changes)
    }

    /// The changes that remove the plugin and the ADR 0012 setup, after every
    /// check. `force` also removes a marketplace from another source, an
    /// edited old skill and a foreign entry.
    pub fn plan_uninstall(&self, home: &Path, force: bool) -> Result<Vec<Change>, LeadSetupError> {
        let dir = plugin_dir(home);
        let state = plugin_state(&self.config, &dir)?;
        let migration = plan_migration(&self.config, force)?;
        let mut changes = Vec::new();
        if state.installed.is_some() {
            changes.push(Change::UninstallPlugin);
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
        changes.extend(migration);
        self.check_claude_runs(&changes)?;
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
        let cfg = &self.config;
        match change {
            Change::WritePlugin { dir, files } => self.write_plugin(dir, files),
            Change::RemovePluginFiles(dir) => std::fs::remove_dir_all(dir)
                .map_err(|e| format!("cannot remove {}: {e}", dir.display())),
            Change::RemoveSkill(path) => remove_skill(path),
            Change::AddMarketplace(dir) => self.run_claude(&plugin::marketplace_add(cfg, dir)),
            Change::UpdateMarketplace => self.run_claude(&plugin::marketplace_update(cfg)),
            Change::RemoveMarketplace => self.run_claude(&plugin::marketplace_remove(cfg)),
            Change::InstallPlugin => self.run_claude(&plugin::plugin_verb(cfg, "install")),
            Change::UpdatePlugin => self.run_claude(&plugin::plugin_verb(cfg, "update")),
            Change::EnablePlugin => self.run_claude(&plugin::plugin_verb(cfg, "enable")),
            Change::UninstallPlugin => self.run_claude(&plugin::plugin_verb(cfg, "uninstall")),
            Change::RemoveMcp(_) => self.run_claude(&mcp::remove_command(cfg)),
        }
    }

    /// Runs `claude plugin --help` when a change needs `claude`, so a missing
    /// `claude`, or one without the `plugin` command, is refused before the
    /// first write.
    fn check_claude_runs(&self, changes: &[Change]) -> Result<(), LeadSetupError> {
        if !changes.iter().any(Change::is_claude) {
            return Ok(());
        }
        self.run_claude(&plugin::probe_command(&self.config))
            .map_err(|detail| {
                LeadSetupError::Refused(format!(
                    "cannot run `{} plugin`, which registers the teamwire plugin: {detail}",
                    self.config.claude_bin
                ))
            })
    }

    fn run_claude(&self, cmd: &teamwire_core::Command) -> Result<(), String> {
        let out = self
            .platform
            .run(cmd, CLAUDE_TIMEOUT)
            .map_err(|e| e.to_string())?;
        if out.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let said = if stderr.is_empty() { stdout } else { stderr };
        Err(if out.timed_out {
            format!("timed out after {}s", CLAUDE_TIMEOUT.as_secs())
        } else {
            format!("exit {}: {said}", out.exit_code)
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

/// The entry found, for the change list.
fn describe(state: &McpState) -> String {
    match state {
        McpState::Outdated(found) => found.to_string(),
        McpState::Foreign(found) => found.clone(),
        McpState::Missing => "none".to_owned(),
        McpState::Current => "current".to_owned(),
    }
}

/// Removes the skill file, then its directory when nothing else is in it,
/// so files the user put beside the skill stay.
fn remove_skill(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot remove {}: {e}", path.display())),
    }
    if let Some(dir) = path.parent() {
        // Fails when the directory holds other files, which is the point.
        std::fs::remove_dir(dir).ok();
    }
    Ok(())
}
