//! Claude Code's side of the lead-setup seam (ADR 0031): the plugin's plan
//! and apply behind `LeadSetup`, and the doctor checks of the plugin, of the
//! ADR 0012 leftovers, of a second `teamwire` server, and of the flags a
//! member's start passes.

use std::path::Path;

use teamwire_core::lead_setup::{DoctorCheck, Probe, apply_in_order};
use teamwire_core::{
    ApplyError, CheckOutcome, LeadChange, LeadCommand, LeadPlan, LeadSetup, LeadSetupError,
    LeadStatus, LeadTarget,
};

use super::plugin::{PLUGIN_ID, plugin_state};
use super::{
    Change, McpEntry, McpState, SkillState, edited_skill, mcp_state, shadowing_entries, skill_state,
};
use crate::ClaudeCode;

const INSTALL: &str = "run `teamwire install`";
const FORCE: &str = "run `teamwire install --force` to remove it";
const LOOSE: &str = "not compared, because this binary is a build under target/";

/// The flags teamwire passes when it starts a member (`launch.rs`). A
/// `claude` whose help does not list one may not support it; the launch
/// itself is the real test.
const CLAUDE_FLAGS: [&str; 8] = [
    "--session-id",
    "--resume",
    "--permission-mode",
    "--append-system-prompt-file",
    "--settings",
    "--mcp-config",
    "--model",
    "--effort",
];

/// The MCP entry Claude Code's plugin runs for `command`.
fn entry(command: &LeadCommand) -> McpEntry {
    McpEntry::lead(&command.bin, command.home.as_deref())
}

impl ClaudeCode {
    fn plan(&self, changes: Vec<Change>) -> LeadPlan {
        let note = changes.iter().any(Change::is_claude).then_some(
            "Claude Code loads plugins at start: new sessions see the change, open ones do not",
        );
        LeadPlan {
            changes: changes
                .into_iter()
                .map(|c| LeadChange::new(c.to_string(), c.step(&self.config)))
                .collect(),
            note,
        }
    }

    fn plugin_check(&self, target: &LeadTarget<'_>, want: &McpEntry) -> CheckOutcome {
        let loose = !target.compare_files;
        match self.config.plugin_problems(want, target.compare_files) {
            Err(e) => CheckOutcome::Fail(e.to_string()),
            Ok(problems) if !problems.is_empty() => {
                CheckOutcome::Fail(format!("{}; {INSTALL}", problems.join("; ")))
            }
            Ok(_) if loose => {
                CheckOutcome::Pass(format!("`{PLUGIN_ID}` is installed and enabled; {LOOSE}"))
            }
            // Installing here would repoint the user's one plugin at this home.
            Ok(_) => match self.config.plugin_other_home(want) {
                Ok(Some(theirs)) => CheckOutcome::Warn(format!(
                    "installed for state directory {}; this run uses {}",
                    theirs.as_deref().unwrap_or("~/.teamwire (the default)"),
                    target.home.display()
                )),
                Err(e) => CheckOutcome::Fail(e.to_string()),
                Ok(None) => CheckOutcome::Pass(format!("installed and enabled: {want}")),
            },
        }
    }

    /// The skill and MCP entry that ADR 0012 wrote must be gone, so the lead
    /// does not see two copies of the skill and two sets of tools.
    fn leftover_check(&self) -> CheckOutcome {
        let mut found = Vec::new();
        let mut needs_force = false;
        match skill_state(&self.config) {
            Err(e) => return CheckOutcome::Fail(e.to_string()),
            Ok((_, SkillState::Missing)) => {}
            Ok((path, SkillState::Edited)) => {
                needs_force = true;
                found.push(edited_skill(&path));
            }
            Ok((path, _)) => found.push(format!("the old skill at {}", path.display())),
        }
        // Any entry will do: the question is only whether one is there.
        let any = McpEntry::lead(Path::new("/"), None);
        match mcp_state(&self.config, &any) {
            Err(e) => return CheckOutcome::Fail(e.to_string()),
            Ok(McpState::Missing) => {}
            Ok(McpState::Foreign(found_entry)) => {
                needs_force = true;
                found.push(format!("the user MCP server `teamwire` ({found_entry})"));
            }
            Ok(McpState::Current | McpState::Outdated(_)) => {
                found.push("the old user MCP server `teamwire`".to_owned());
            }
        }
        if found.is_empty() {
            CheckOutcome::Pass("no skill or user MCP server left from the old install".to_owned())
        } else {
            let fix = if needs_force { FORCE } else { INSTALL };
            CheckOutcome::Fail(format!("{} remains; {fix}", found.join(" and ")))
        }
    }

    /// Entries in the local or project scope start a second `teamwire`
    /// server beside the plugin's in that directory only, so they warn rather
    /// than fail.
    fn shadow_check(&self, dir: &Path, want: &McpEntry) -> CheckOutcome {
        match shadowing_entries(&self.config, dir, want) {
            Err(e) => CheckOutcome::Warn(e.to_string()),
            Ok(found) if found.is_empty() => CheckOutcome::Pass(format!(
                "no local or project `teamwire` entry in {}",
                dir.display()
            )),
            Ok(found) => CheckOutcome::Warn(
                found
                    .iter()
                    .map(|s| {
                        format!(
                            "{} has `teamwire` = {}, which Claude Code starts as a second \
                             server beside the plugin's here; remove it",
                            s.place, s.entry
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
        }
    }
}

/// Claude Code writes a flag as `--name-suffix` or, in a description, as
/// `--name[-suffix]` (2.1.301 does this for `--append-system-prompt-file`).
fn lists_flag(help: &str, flag: &str) -> bool {
    help.contains(flag)
        || flag
            .rsplit_once('-')
            .is_some_and(|(base, suffix)| help.contains(&format!("{base}[-{suffix}]")))
}

impl LeadSetup for ClaudeCode {
    fn program(&self) -> &str {
        &self.config.claude_bin
    }

    fn label(&self) -> &'static str {
        "Claude Code"
    }

    fn default_lead(&self) -> bool {
        true
    }

    fn status(&self, target: &LeadTarget<'_>) -> Result<LeadStatus, LeadSetupError> {
        if plugin_state(&self.config, Path::new(""))?
            .installed
            .is_none()
        {
            return Ok(LeadStatus::Missing);
        }
        let problems = self
            .config
            .plugin_problems(&entry(target.command), target.compare_files)?;
        Ok(if problems.is_empty() {
            LeadStatus::Current
        } else {
            LeadStatus::Stale(problems)
        })
    }

    fn plan_install(
        &self,
        target: &LeadTarget<'_>,
        force: bool,
    ) -> Result<LeadPlan, LeadSetupError> {
        // The skill is compiled into this crate, so `shared_skill` is unused.
        let changes = Self::plan_install(self, &entry(target.command), target.home, force)?;
        Ok(self.plan(changes))
    }

    fn plan_uninstall(&self, home: &Path, force: bool) -> Result<LeadPlan, LeadSetupError> {
        let changes = Self::plan_uninstall(self, home, force)?;
        Ok(self.plan(changes))
    }

    fn apply(&self, changes: &[LeadChange]) -> Result<(), Box<ApplyError>> {
        apply_in_order(changes, |c| self.make(c.step()))
    }

    fn doctor_checks(&self, target: Result<&LeadTarget<'_>, &str>, dir: &Path) -> Vec<DoctorCheck> {
        // The leftover check needs no entry, so it runs even without one.
        match target {
            Ok(target) => {
                let want = entry(target.command);
                vec![
                    ("teamwire plugin", self.plugin_check(target, &want)),
                    ("old lead setup", self.leftover_check()),
                    ("lead MCP server scope", self.shadow_check(dir, &want)),
                ]
            }
            Err(e) => vec![
                ("teamwire plugin", CheckOutcome::Fail(e.to_owned())),
                ("old lead setup", self.leftover_check()),
            ],
        }
    }

    fn program_checks(&self, probe: Probe<'_>) -> Vec<DoctorCheck> {
        let outcome = match probe(&["--help"]) {
            Ok(help) => {
                let missing: Vec<&str> = CLAUDE_FLAGS
                    .into_iter()
                    .filter(|f| !lists_flag(&help, f))
                    .collect();
                if missing.is_empty() {
                    CheckOutcome::Pass("the help text lists every flag teamwire passes".to_owned())
                } else {
                    CheckOutcome::Warn(format!(
                        "`claude --help` does not list {}; teamwire passes them when it \
                         starts a member, so this claude may be too old or too new",
                        missing.join(", ")
                    ))
                }
            }
            Err(e) => CheckOutcome::Warn(format!("`claude --help` failed: {e}")),
        };
        vec![("claude flags", outcome)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_in_brackets_counts_as_listed() {
        assert!(lists_flag(
            "--append-system-prompt[-file]",
            "--append-system-prompt-file"
        ));
        assert!(lists_flag("--model <m>", "--model"));
        assert!(!lists_flag("--resume", "--model"));
    }
}
