//! The lead setup checks of `teamwire doctor` (ADR 0012): the skill and the
//! lead's MCP entry, each compared with what this binary would install, and
//! any `teamwire` entry in a scope that hides the user one. Reads only.
//! Kept apart from `doctor.rs` and free of its types, so the doctor tests
//! compile this file as it is.

use std::path::Path;

use crate::wiring::{
    ClaudeConfig, McpEntry, McpState, SkillState, edited_skill, mcp_state, shadowing_entries,
    skill_state,
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
    /// True when this binary is a cargo build under `target/`. Its skill
    /// text and path differ from the installed binary's by design, so the
    /// checks then ask only whether both items are teamwire's.
    pub(crate) running_a_build: bool,
    /// The directory whose local and project scopes are checked.
    pub(crate) dir: &'a Path,
}

/// True when the binary sits under a `target/` directory: a cargo build,
/// not an installed binary.
pub(crate) fn is_build_output(bin: &Path) -> bool {
    bin.components().any(|c| c.as_os_str() == "target")
}

const INSTALL: &str = "run `teamwire install`";
const FORCE: &str = "run `teamwire install --force` to replace it";
const LOOSE: &str = "not compared, because this binary is a build under target/";

fn skill_check(inputs: &LeadInputs<'_>) -> Outcome {
    let (path, state) = match skill_state(inputs.claude) {
        Ok(found) => found,
        Err(e) => return Outcome::Fail(e.to_string()),
    };
    let at = path.display();
    match state {
        SkillState::Current => Outcome::Pass(format!("installed at {at}")),
        SkillState::Outdated if inputs.running_a_build => {
            Outcome::Pass(format!("teamwire's skill at {at}; {LOOSE}"))
        }
        SkillState::Outdated => Outcome::Fail(format!("{at} is another version; {INSTALL}")),
        SkillState::Missing => Outcome::Fail(format!("not installed at {at}; {INSTALL}")),
        SkillState::Edited => Outcome::Fail(format!("{}; {FORCE}", edited_skill(&path))),
    }
}

fn mcp_check(inputs: &LeadInputs<'_>, want: &McpEntry) -> Outcome {
    match mcp_state(inputs.claude, want) {
        Err(e) => Outcome::Fail(e.to_string()),
        Ok(McpState::Current) => Outcome::Pass(format!("registered: {want}")),
        Ok(McpState::Outdated(found)) if inputs.running_a_build => {
            Outcome::Pass(format!("registered: {found}; {LOOSE}"))
        }
        Ok(McpState::Outdated(found)) => {
            Outcome::Fail(format!("registered as `{found}`, not `{want}`; {INSTALL}"))
        }
        Ok(McpState::Missing) => Outcome::Fail(format!("no user MCP server `teamwire`; {INSTALL}")),
        Ok(McpState::Foreign(found)) => {
            Outcome::Fail(format!("`teamwire` is another server ({found}); {FORCE}"))
        }
    }
}

/// Entries in the local or project scope run in place of the user one in
/// that directory only, so they warn rather than fail.
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
                        "{} has `teamwire` = {}, which Claude Code starts in place of the \
                         user entry here; remove it",
                        s.place, s.entry
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        ),
    }
}

/// Each check's name and outcome. The skill check does not need the entry,
/// so it runs even when the entry cannot be worked out.
pub(crate) fn run(inputs: &LeadInputs<'_>) -> Vec<(&'static str, Outcome)> {
    let mut checks = vec![("teamwire skill", skill_check(inputs))];
    match inputs.entry {
        Ok(want) => {
            checks.push(("lead MCP server", mcp_check(inputs, want)));
            checks.push(("lead MCP server scope", shadow_check(inputs, want)));
        }
        Err(e) => checks.push(("lead MCP server", Outcome::Fail(e.clone()))),
    }
    checks
}
