//! The contract text a member's system prompt carries. The words live in
//! `contracts/*.md`, so a test can check that every recorded rule is there
//! and a harness can render them its own way (LCH-17). The text names only
//! `teamwire` commands and tools.

use std::path::Path;

use teamwire_core::{LaunchSpec, MemberKind, Role, TeamName};

const HEAD: &str = include_str!("../contracts/head.md");
const KIND_IMPLEMENTER: &str = include_str!("../contracts/kind_implementer.md");
const KIND_REVIEWER: &str = include_str!("../contracts/kind_reviewer.md");
const KIND_INVESTIGATOR: &str = include_str!("../contracts/kind_investigator.md");
const KIND_CLOSER: &str = include_str!("../contracts/kind_closer.md");
const KIND_PLAN_REVIEWER: &str = include_str!("../contracts/kind_plan_reviewer.md");
const SCOPE_HEAD: &str = include_str!("../contracts/scope_head.md");
const SCOPE_OWNS: &str = include_str!("../contracts/scope_owns.md");
const SCOPE_CHECK: &str = include_str!("../contracts/scope_check.md");
const TALKING_SILENT: &str = include_str!("../contracts/talking_silent.md");
const TALKING_HEAD: &str = include_str!("../contracts/talking_head.md");
const TALKING_INFO: &str = include_str!("../contracts/talking_info.md");
const TALKING_TEAMS: &str = include_str!("../contracts/talking_teams.md");
const TALKING_TAIL: &str = include_str!("../contracts/talking_tail.md");
const TOGETHER: &str = include_str!("../contracts/together.md");
const TOGETHER_ASK_MESSAGE: &str = include_str!("../contracts/together_ask_message.md");
const TOGETHER_ASK_REPORT: &str = include_str!("../contracts/together_ask_report.md");
const HOW_TO_WORK: &str = include_str!("../contracts/how_to_work.md");
const FINISH: &str = include_str!("../contracts/finish.md");

/// What one contract is written for.
#[derive(Debug, Clone, Copy)]
pub struct ContractParts<'a> {
    pub team: &'a TeamName,
    pub role: &'a Role,
    pub kind: MemberKind,
    /// The member's own report file.
    pub report_file: &'a Path,
    /// The team's `outbox/` directory, which a member must never write into.
    pub outbox_dir: &'a Path,
    /// The launching `teamwire` binary. A member's shell may have no
    /// `teamwire` on its PATH, so every command the contract names starts
    /// with this path.
    pub teamwire_bin: &'a Path,
    pub check: Option<&'a str>,
    pub owns: &'a [String],
}

fn kind_text(kind: MemberKind) -> &'static str {
    match kind {
        MemberKind::Implementer => KIND_IMPLEMENTER,
        MemberKind::Reviewer => KIND_REVIEWER,
        MemberKind::Investigator => KIND_INVESTIGATOR,
        MemberKind::Closer => KIND_CLOSER,
        MemberKind::PlanReviewer => KIND_PLAN_REVIEWER,
    }
}

fn scope(check: Option<&str>, owns: &[String]) -> String {
    if check.is_none() && owns.is_empty() {
        return String::new();
    }
    let mut out = SCOPE_HEAD.to_owned();
    if !owns.is_empty() {
        let lines: Vec<String> = owns.iter().map(|o| format!("    {o}")).collect();
        out.push_str(&SCOPE_OWNS.replace("{owns}", &lines.join("\n")));
    }
    if let Some(check) = check {
        out.push_str(&SCOPE_CHECK.replace("{check}", check));
    }
    out
}

/// Every member that talks sends every kind of information, to its own team
/// and to other teams (MSG-1, MSG-20); a quiet kind talks to no one.
fn talking(kind: MemberKind) -> String {
    match kind {
        MemberKind::Reviewer | MemberKind::Closer | MemberKind::PlanReviewer => {
            TALKING_SILENT.to_owned()
        }
        MemberKind::Implementer | MemberKind::Investigator => {
            [TALKING_HEAD, TALKING_INFO, TALKING_TAIL, TALKING_TEAMS].concat()
        }
    }
}

/// How a member reaches the parent with a question. A quiet kind may send no
/// message (MSG-3), so its question goes in its report file.
fn ask_the_parent(kind: MemberKind) -> String {
    let ask = match kind {
        MemberKind::Reviewer | MemberKind::Closer | MemberKind::PlanReviewer => TOGETHER_ASK_REPORT,
        MemberKind::Implementer | MemberKind::Investigator => TOGETHER_ASK_MESSAGE,
    };
    [TOGETHER, ask].concat()
}

/// The whole contract for one member. The task is not here: it is the
/// member's first message (LCH-19).
pub fn render_contract(p: &ContractParts<'_>) -> String {
    let bin = LaunchSpec::shell_word(&p.teamwire_bin.display().to_string());
    // The placeholder is filled in the fixed text before any path or check
    // is added, so text from the spec that holds `{teamwire}` stays as written.
    let head = HEAD
        .replace("{teamwire}", &bin)
        .replace("{team}", p.team.as_str())
        .replace("{role}", p.role.as_str())
        .replace("{report_file}", &p.report_file.display().to_string());
    let finish = FINISH
        .replace("{teamwire}", &bin)
        .replace("{outbox_dir}", &p.outbox_dir.display().to_string());
    [
        head,
        kind_text(p.kind).to_owned(),
        scope(p.check, p.owns),
        talking(p.kind)
            .replace("{teamwire}", &bin)
            .replace("{team}", p.team.as_str()),
        ask_the_parent(p.kind),
        HOW_TO_WORK.to_owned(),
        finish,
    ]
    .concat()
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_findings;
