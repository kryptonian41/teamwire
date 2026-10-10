//! The lead setup: the `teamwire` skill and the lead's MCP server entry in
//! Claude Code's user configuration (ADR 0012).
//!
//! Install and uninstall run in two halves. `plan_*` reads both items, runs
//! every check, and returns the changes; nothing is written. `apply` makes
//! them in order and stops at the first failure. A caller that only plans
//! has a dry run.

mod mcp;
mod shadow;
mod skill;

pub use mcp::{McpEntry, McpState};
pub use shadow::{Shadow, shadowing_entries};
pub use skill::{SKILL_TEXT, SkillState};

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

/// Both items of the lead setup, as they are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadSetupState {
    pub skill_path: PathBuf,
    pub skill: SkillState,
    pub mcp: McpState,
}

/// One change install or uninstall makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Write this binary's skill, stamped, over whatever is there.
    WriteSkill(PathBuf),
    RemoveSkill(PathBuf),
    AddMcp(McpEntry),
    /// `claude mcp add` refuses a name that exists, so a replacement is a
    /// remove followed by an add.
    ReplaceMcp {
        old: String,
        new: McpEntry,
    },
    RemoveMcp(String),
}

impl Change {
    /// True for a change to the MCP entry, which needs `claude`.
    pub fn is_mcp(&self) -> bool {
        matches!(
            self,
            Self::AddMcp(_) | Self::ReplaceMcp { .. } | Self::RemoveMcp(_)
        )
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WriteSkill(p) => write!(f, "write the teamwire skill to {}", p.display()),
            Self::RemoveSkill(p) => write!(f, "remove the teamwire skill at {}", p.display()),
            Self::AddMcp(e) => write!(f, "add the user MCP server `teamwire`: {e}"),
            Self::ReplaceMcp { old, new } => write!(
                f,
                "replace the user MCP server `teamwire` ({old}) with: {new}"
            ),
            Self::RemoveMcp(old) => write!(f, "remove the user MCP server `teamwire` ({old})"),
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
    /// True when the failure came after `claude mcp remove` succeeded, so
    /// the old entry is gone and the new one was not added.
    pub entry_removed: bool,
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

/// The skill file's path and state. Reads only.
pub fn skill_state(config: &ClaudeConfig) -> Result<(PathBuf, SkillState), LeadSetupError> {
    let path = skill::path(&config.claude_dir);
    let state = skill::read(&path, SKILL_TEXT)?;
    Ok((path, state))
}

/// The user-scope MCP entry compared with `want`. Reads only.
pub fn mcp_state(config: &ClaudeConfig, want: &McpEntry) -> Result<McpState, LeadSetupError> {
    mcp::read(&config.claude_json, want)
}

/// Reads both items, comparing the MCP entry with `want`. Reads only, so
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

impl ClaudeCode {
    /// The changes that make the lead setup this binary's, after every
    /// check. `force` replaces an edited skill and a foreign entry.
    pub fn plan_install(
        &self,
        want: &McpEntry,
        force: bool,
    ) -> Result<Vec<Change>, LeadSetupError> {
        let state = lead_setup_state(&self.config, want)?;
        let mut changes = Vec::new();
        match state.skill {
            SkillState::Current => {}
            SkillState::Edited if !force => {
                return Err(refused(&edited_skill(&state.skill_path), "replace"));
            }
            SkillState::Missing | SkillState::Outdated | SkillState::Edited => {
                changes.push(Change::WriteSkill(state.skill_path.clone()));
            }
        }
        match &state.mcp {
            McpState::Current => {}
            McpState::Missing => changes.push(Change::AddMcp(want.clone())),
            McpState::Foreign(found) if !force => {
                return Err(refused(&foreign_entry(found), "replace"));
            }
            McpState::Outdated(_) | McpState::Foreign(_) => {
                changes.push(Change::ReplaceMcp {
                    old: describe(&state.mcp),
                    new: want.clone(),
                });
            }
        }
        self.check_claude_runs(&changes)?;
        Ok(changes)
    }

    /// The changes that remove the lead setup, after every check. `force`
    /// also removes an edited skill and a foreign entry.
    pub fn plan_uninstall(&self, force: bool) -> Result<Vec<Change>, LeadSetupError> {
        // Any lead entry will do: uninstall asks only "is it teamwire's".
        let any = McpEntry::lead(Path::new("teamwire"), None);
        let state = lead_setup_state(&self.config, &any)?;
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
        self.check_claude_runs(&changes)?;
        Ok(changes)
    }

    /// Makes `changes` in order, stopping at the first failure.
    pub fn apply_lead_setup(&self, changes: &[Change]) -> Result<(), Box<ApplyError>> {
        let mut done = Vec::new();
        for change in changes {
            let result = match change {
                Change::WriteSkill(path) => self.write_skill(path).map_err(|d| (d, false)),
                Change::RemoveSkill(path) => remove_skill(path).map_err(|d| (d, false)),
                Change::AddMcp(entry) => self
                    .run_claude(&mcp::add_command(&self.config, entry))
                    .map_err(|d| (d, false)),
                Change::RemoveMcp(_) => self
                    .run_claude(&mcp::remove_command(&self.config))
                    .map_err(|d| (d, false)),
                Change::ReplaceMcp { new, .. } => self
                    .run_claude(&mcp::remove_command(&self.config))
                    .map_err(|d| (d, false))
                    .and_then(|()| {
                        self.run_claude(&mcp::add_command(&self.config, new))
                            .map_err(|d| (d, true))
                    }),
            };
            if let Err((detail, entry_removed)) = result {
                return Err(Box::new(ApplyError {
                    done,
                    failed: change.clone(),
                    detail,
                    entry_removed,
                }));
            }
            done.push(change.clone());
        }
        Ok(())
    }

    /// Runs `claude --version` when a change needs `claude`, so a missing
    /// `claude` is refused before the skill is written.
    fn check_claude_runs(&self, changes: &[Change]) -> Result<(), LeadSetupError> {
        if !changes.iter().any(Change::is_mcp) {
            return Ok(());
        }
        self.run_claude(&mcp::version_command(&self.config))
            .map_err(|detail| {
                LeadSetupError::Refused(format!(
                    "cannot run `{}`, which changes the MCP server entry: {detail}",
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

    fn write_skill(&self, path: &Path) -> Result<(), String> {
        let text = skill::stamp(SKILL_TEXT)
            .ok_or_else(|| "the compiled-in skill has no frontmatter".to_owned())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        self.platform
            .write_atomic(path, text.as_bytes())
            .map_err(|e| e.to_string())
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
