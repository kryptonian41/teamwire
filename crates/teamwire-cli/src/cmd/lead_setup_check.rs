//! The lead setup checks of `teamwire doctor` (ADR 0018): the teamwire plugin,
//! compared with what this binary would install, any skill or MCP entry left
//! from ADR 0012, and any `teamwire` entry in a scope that adds a second
//! server. Reads only. Kept apart from `doctor.rs` and free of its types, so
//! the doctor tests compile this file as it is.

use std::path::Path;

use crate::wiring::{
    ClaudeConfig, CodexCommand, CodexPluginStatus, CodexSetup, McpEntry, McpState, SKILL_TEXT,
    SkillState, edited_skill, mcp_state, shadowing_entries, skill_state,
};

/// One check's verdict and its detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Pass(String),
    Warn(String),
    Fail(String),
}

/// What the checks look at.
pub(crate) struct LeadInputs<'a> {
    pub(crate) claude: &'a ClaudeConfig,
    /// The entry `teamwire install` would register, or why there is none.
    pub(crate) entry: &'a Result<McpEntry, String>,
    /// True when this binary is a cargo build under `target/`. Its path and
    /// skill text differ from the installed binary's by design, so the
    /// plugin check then asks only whether the plugin is installed and on.
    pub(crate) running_a_build: bool,
    /// The directory whose local and project scopes are checked.
    pub(crate) dir: &'a Path,
    /// The Codex adapter, when `codex` is on PATH; without it Codex has no
    /// check.
    pub(crate) codex: Option<&'a CodexSetup>,
    /// The state directory, which holds the Codex plugin's files.
    pub(crate) home: &'a Path,
}

/// True when the binary sits under a `target/` directory: a cargo build,
/// not an installed binary.
pub(crate) fn is_build_output(bin: &Path) -> bool {
    bin.components().any(|c| c.as_os_str() == "target")
}

const INSTALL: &str = "run `teamwire install`";
const FORCE: &str = "run `teamwire install --force` to remove it";
const LOOSE: &str = "not compared, because this binary is a build under target/";

fn plugin_check(inputs: &LeadInputs<'_>, want: &McpEntry) -> Outcome {
    match inputs.claude.plugin_problems(want, !inputs.running_a_build) {
        Err(e) => Outcome::Fail(e.to_string()),
        Ok(problems) if !problems.is_empty() => {
            Outcome::Fail(format!("{}; {INSTALL}", problems.join("; ")))
        }
        Ok(_) if inputs.running_a_build => Outcome::Pass(format!(
            "`teamwire@teamwire` is installed and enabled; {LOOSE}"
        )),
        Ok(_) => Outcome::Pass(format!("installed and enabled: {want}")),
    }
}

/// The skill and MCP entry that ADR 0012 wrote must be gone, so the lead does
/// not see two copies of the skill and two sets of tools.
fn leftover_check(inputs: &LeadInputs<'_>) -> Outcome {
    let mut found = Vec::new();
    let mut needs_force = false;
    match skill_state(inputs.claude) {
        Err(e) => return Outcome::Fail(e.to_string()),
        Ok((_, SkillState::Missing)) => {}
        Ok((path, SkillState::Edited)) => {
            needs_force = true;
            found.push(edited_skill(&path));
        }
        Ok((path, _)) => found.push(format!("the old skill at {}", path.display())),
    }
    // Any entry will do: the question is only whether one is there.
    let any = McpEntry::lead(Path::new("/"), None);
    match mcp_state(inputs.claude, &any) {
        Err(e) => return Outcome::Fail(e.to_string()),
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
        Outcome::Pass("no skill or user MCP server left from the old install".to_owned())
    } else {
        let fix = if needs_force { FORCE } else { INSTALL };
        Outcome::Fail(format!("{} remains; {fix}", found.join(" and ")))
    }
}

/// Entries in the local or project scope start a second `teamwire` server
/// beside the plugin's in that directory only, so they warn rather than fail.
fn shadow_check(inputs: &LeadInputs<'_>, want: &McpEntry) -> Outcome {
    match shadowing_entries(inputs.claude, inputs.dir, want) {
        Err(e) => Outcome::Warn(e.to_string()),
        Ok(found) if found.is_empty() => Outcome::Pass(format!(
            "no local or project `teamwire` entry in {}",
            inputs.dir.display()
        )),
        Ok(found) => Outcome::Warn(
            found
                .iter()
                .map(|s| {
                    format!(
                        "{} has `teamwire` = {}, which Claude Code starts as a second server \
                         beside the plugin's here; remove it",
                        s.place, s.entry
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        ),
    }
}

/// The command the Codex plugin runs, from the entry install would register:
/// the same binary, and `--home` when the entry names one.
fn codex_command(want: &McpEntry) -> CodexCommand {
    let home = match want.args.as_slice() {
        [flag, home, ..] if flag == "--home" => Some(Path::new(home).to_path_buf()),
        _ => None,
    };
    CodexCommand {
        bin: Path::new(&want.command).to_path_buf(),
        home,
    }
}

/// A missing plugin is fine: Codex is optional. A stale one is a warning.
fn codex_check(inputs: &LeadInputs<'_>, codex: &CodexSetup, want: &McpEntry) -> Outcome {
    const ADD: &str = "run `teamwire install --harness codex`";
    let status = codex.plugin_status(
        &codex_command(want),
        SKILL_TEXT,
        inputs.home,
        !inputs.running_a_build,
    );
    match status {
        Err(e) => Outcome::Warn(e.to_string()),
        Ok(CodexPluginStatus::Missing) => {
            Outcome::Pass(format!("not installed; to lead from Codex, {ADD}"))
        }
        Ok(CodexPluginStatus::Stale(problems)) => {
            Outcome::Warn(format!("{}; {ADD}", problems.join("; ")))
        }
        Ok(CodexPluginStatus::Current) => Outcome::Pass("installed".to_owned()),
    }
}

/// Each check's name and outcome. The leftover check does not need the
/// entry, so it runs even when the entry cannot be worked out.
pub(crate) fn run(inputs: &LeadInputs<'_>) -> Vec<(&'static str, Outcome)> {
    let mut checks = Vec::new();
    match inputs.entry {
        Ok(want) => {
            checks.push(("teamwire plugin", plugin_check(inputs, want)));
            checks.push(("old lead setup", leftover_check(inputs)));
            checks.push(("lead MCP server scope", shadow_check(inputs, want)));
            if let Some(codex) = inputs.codex {
                checks.push(("codex plugin", codex_check(inputs, codex, want)));
            }
        }
        Err(e) => {
            checks.push(("teamwire plugin", Outcome::Fail(e.clone())));
            checks.push(("old lead setup", leftover_check(inputs)));
        }
    }
    checks
}
