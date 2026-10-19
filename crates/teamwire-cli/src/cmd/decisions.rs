//! `teamwire decisions`: the "Decisions I made" blocks of members' reports.
//! Reads only.

use std::process::ExitCode;

use teamwire_core::TeamName;
use teamwire_report::{DecisionsError, DecisionsQuery, DecisionsView, read_decisions};

use crate::cmd::send::{fail, home_or_refuse, say, warn};
use crate::wiring::Wiring;
use crate::{DecisionsArgs, Global, USAGE};

/// Refused by a rule, or nothing could be read (docs/api/cli.md).
const REFUSED: u8 = 1;
const USAGE_LINE: &str = "name a team or give --ref or --grep; see teamwire decisions --help";

pub(crate) fn run(args: &DecisionsArgs, global: &Global, _wiring: &Wiring) -> ExitCode {
    let DecisionsArgs {
        team,
        role,
        grep,
        reference,
        json,
    } = args;
    let team = match team.as_deref().map(TeamName::parse).transpose() {
        Ok(team) => team,
        Err(e) => return fail("decisions", &e.to_string(), USAGE),
    };
    let home = match home_or_refuse("decisions", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let query = DecisionsQuery {
        team,
        role: role.clone(),
        grep: grep.clone(),
        reference: reference.clone(),
        // The person at the terminal reads every team.
        scope: None,
    };
    let view = match read_decisions(&home, &query) {
        Ok(view) => view,
        Err(DecisionsError::NoFilter) => return fail("decisions", USAGE_LINE, USAGE),
        Err(e) => return fail("decisions", &e.to_string(), REFUSED),
    };
    for error in &view.errors {
        warn(error);
    }
    if *json {
        match serde_json::to_string(&view) {
            Ok(text) => say(&format!("{text}\n")),
            Err(e) => return fail("decisions", &e.to_string(), REFUSED),
        }
    } else {
        say(&render(&view));
    }
    if view.teams.is_empty() && !view.errors.is_empty() {
        return ExitCode::from(REFUSED);
    }
    ExitCode::SUCCESS
}

/// One heading per team, then each member's blocks.
fn render(view: &DecisionsView) -> String {
    if view.teams.is_empty() {
        return "no decisions found\n".to_owned();
    }
    let mut lines: Vec<String> = Vec::new();
    for team in &view.teams {
        let state = if team.live { "running" } else { "archived" };
        let refs: Vec<String> = team.refs.iter().map(ToString::to_string).collect();
        let refs = if refs.is_empty() {
            String::new()
        } else {
            format!(" [{}]", refs.join(", "))
        };
        lines.push(format!("== {} ({state}): {}{refs}", team.team, team.title));
        for member in &team.members {
            lines.push(format!("-- {} ({})", member.role, member.kind));
            if member.report.is_none() {
                lines.push("(no report)".to_owned());
            }
            for block in &member.blocks {
                lines.push(format!("{} (line {})", block.heading, block.line));
                if !block.text.is_empty() {
                    lines.push(block.text.clone());
                }
            }
        }
    }
    lines.push(String::new());
    lines.join("\n")
}
