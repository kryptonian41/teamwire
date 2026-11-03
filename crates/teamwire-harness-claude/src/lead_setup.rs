//! The lead setup: the `teamwire` plugin in Claude Code (ADR 0018), and the
//! removal of the skill and MCP entry that ADR 0012 wrote.
//!
//! Install and uninstall run in two halves. `plan_*` reads both setups, runs
//! every check, and returns the changes; nothing is written. `apply` makes
//! them in order and stops at the first failure. A caller that only plans
//! has a dry run.

mod mcp;
mod plugin;
mod seam;
mod shadow;
mod skill;

pub use mcp::{McpEntry, McpState};
pub use shadow::{Shadow, shadowing_entries};
pub use skill::{SKILL_TEXT, SkillState};
pub use teamwire_core::LeadSetupError;

use std::fmt;
use std::path::{Path, PathBuf};

use teamwire_core::lead_setup::{apply_in_order, make_step, run_program};
use teamwire_core::{ApplyError, LeadStep};

use plugin::{MarketplaceState, PLUGIN_ID, PluginFiles, plugin_dir, plugin_state};

use crate::{ClaudeCode, ClaudeConfig};

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

    /// What making the change does.
    pub(crate) fn step(&self, cfg: &ClaudeConfig) -> LeadStep {
        let run = LeadStep::Run;
        match self {
            Self::WritePlugin { dir, files } => LeadStep::WriteFiles {
                dir: dir.clone(),
                files: files.clone(),
            },
            Self::RemovePluginFiles(dir) => LeadStep::RemoveDir(dir.clone()),
            Self::RemoveSkill(path) => LeadStep::Own(path.clone()),
            Self::AddMarketplace(dir) => run(plugin::marketplace_add(cfg, dir)),
            Self::UpdateMarketplace => run(plugin::marketplace_update(cfg)),
            Self::RemoveMarketplace => run(plugin::marketplace_remove(cfg)),
            Self::InstallPlugin => run(plugin::plugin_verb(cfg, "install")),
            Self::UpdatePlugin => run(plugin::plugin_verb(cfg, "update")),
            Self::EnablePlugin => run(plugin::plugin_verb(cfg, "enable")),
            Self::UninstallPlugin => run(plugin::plugin_verb(cfg, "uninstall")),
            Self::RemoveMcp(_) => run(mcp::remove_command(cfg)),
        }
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
    /// The state directory the installed plugin was made for, when that is
    /// not the one `want` names and its files are what this binary renders
    /// for it: `Some(None)` for the default directory, `Some(Some(dir))` for
    /// a `--home`. Reads only.
    pub fn plugin_other_home(
        &self,
        want: &McpEntry,
    ) -> Result<Option<Option<String>>, LeadSetupError> {
        let Some(root) = plugin_state(self, Path::new(""))?.installed else {
            return Ok(None);
        };
        Ok(installed_for_own_home(&root, want)
            .filter(|e| e.home_args() != want.home_args())
            .map(|e| e.home_args().get(1).cloned()))
    }

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
            Some(root)
                if compare_files
                    && !files.plugin_matches(root)
                    && installed_for_own_home(root, want).is_none() =>
            {
                problems.push(format!(
                    "the installed plugin at {} is another version of teamwire's",
                    root.display()
                ));
            }
            Some(_) => {}
        }
        if state.installed.is_some() && state.enabled == Some(false) {
            problems.push(format!("`{PLUGIN_ID}` is disabled at user scope"));
        }
        Ok(problems)
    }
}

/// The entry the installed plugin at `root` runs: `want`'s binary with the
/// arguments in the plugin's own `.mcp.json`, when `root` holds exactly the
/// files this binary renders for that entry. A plugin installed for another
/// state directory differs from `want` only in `--home`.
fn installed_for_own_home(root: &Path, want: &McpEntry) -> Option<McpEntry> {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(".mcp.json")).ok()?).ok()?;
    let args = doc
        .get("mcpServers")?
        .get("teamwire")?
        .get("args")?
        .as_array()?
        .iter()
        .map(|a| a.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    let entry = McpEntry {
        command: want.command.clone(),
        args,
    };
    let files = PluginFiles::render(Path::new(""), &entry).ok()?;
    files.plugin_matches(root).then_some(entry)
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
            return Err(LeadSetupError::needs_force(
                &edited_skill(&state.skill_path),
                "remove",
            ));
        }
        SkillState::Current | SkillState::Outdated | SkillState::Edited => {
            changes.push(Change::RemoveSkill(state.skill_path.clone()));
        }
    }
    match &state.mcp {
        McpState::Missing => {}
        McpState::Foreign(found) if !force => {
            return Err(LeadSetupError::needs_force(&foreign_entry(found), "remove"));
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
                return Err(LeadSetupError::needs_force(FOREIGN_MARKETPLACE, "replace"));
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
                return Err(LeadSetupError::needs_force(FOREIGN_MARKETPLACE, "remove"));
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
    pub fn apply_lead_setup(&self, changes: &[Change]) -> Result<(), Box<ApplyError<Change>>> {
        apply_in_order(changes, |c| self.make(&c.step(&self.config)))
    }

    /// Makes one step: the old skill's removal here, the rest in core.
    pub(crate) fn make(&self, step: &LeadStep) -> Result<(), String> {
        match step {
            LeadStep::Own(path) => remove_skill(path),
            other @ (LeadStep::WriteFiles { .. } | LeadStep::RemoveDir(_) | LeadStep::Run(_)) => {
                make_step(self.platform.as_ref(), other)
            }
        }
    }

    /// Runs `claude plugin --help` when a change needs `claude`, so a missing
    /// `claude`, or one without the `plugin` command, is refused before the
    /// first write.
    fn check_claude_runs(&self, changes: &[Change]) -> Result<(), LeadSetupError> {
        if !changes.iter().any(Change::is_claude) {
            return Ok(());
        }
        run_program(self.platform.as_ref(), &plugin::probe_command(&self.config))
            .map(drop)
            .map_err(|detail| {
                LeadSetupError::Refused(format!(
                    "cannot run `{} plugin`, which registers the teamwire plugin: {detail}",
                    self.config.claude_bin
                ))
            })
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
/// so files the user put beside the skill stay. A skill folder that is a
/// symlink loses only the link: following it would delete a file in the
/// link's target, which may be the user's own.
fn remove_skill(path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_symlink())
    {
        return std::fs::remove_file(dir)
            .map_err(|e| format!("cannot remove {}: {e}", dir.display()));
    }
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

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the test writes the user's files as the user would, not through the platform"
)]
mod tests {
    use super::*;

    #[test]
    fn a_real_folder_loses_the_file_and_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("skill");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "x").unwrap();
        remove_skill(&dir.join("SKILL.md")).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn a_symlinked_folder_loses_only_the_link() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("mine");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), "keep").unwrap();
        let link = tmp.path().join("skill");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_skill(&link.join("SKILL.md")).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert_eq!(
            std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn a_symlinked_file_in_a_real_folder_loses_only_the_link() {
        let tmp = tempfile::tempdir().unwrap();
        let mine = tmp.path().join("mine.md");
        std::fs::write(&mine, "keep").unwrap();
        let dir = tmp.path().join("skill");
        std::fs::create_dir(&dir).unwrap();
        std::os::unix::fs::symlink(&mine, dir.join("SKILL.md")).unwrap();
        remove_skill(&dir.join("SKILL.md")).unwrap();
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), "keep");
        assert!(!dir.exists());
    }
}
