//! `teamwire status`: the read-only team view.

use std::path::Path;
use std::process::ExitCode;

use teamwire_core::{Config, Liveness, Recipient, Role, TeamName};
use teamwire_store::{Ctx, Store, StoreError};
use teamwire_watch::StatusView;

use crate::cmd::send::{carries_member_identity, home_or_refuse};
use crate::wiring::Wiring;
use crate::{Global, StatusArgs, USAGE};

/// Exit codes of `docs/api/cli.md`.
const REFUSED: u8 = 1;
const STORE_ERROR: u8 = 6;

/// The view of one team, from its database opened read-only.
fn view_of(
    home: &Path,
    team: &TeamName,
    role: Option<&Role>,
    wiring: &Wiring,
) -> Result<StatusView, StoreError> {
    let mut store = Store::open_read_only(home, team)?;
    let config = Config::default();
    let ctx = Ctx {
        home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    Ok(teamwire_watch::status_with(&ctx, role))
}

/// Every directory under `<home>/teams` that names a valid team.
fn team_names(home: &Path) -> Vec<TeamName> {
    let Ok(entries) = std::fs::read_dir(home.join("teams")) else {
        return Vec::new();
    };
    let mut names: Vec<TeamName> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().and_then(|n| TeamName::parse(n).ok()))
        .collect();
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    names
}

fn liveness_text(liveness: Liveness) -> String {
    match liveness {
        Liveness::Alive(state) => format!("alive, {}", state.as_str()),
        Liveness::Dead => "gone".to_owned(),
        Liveness::Unknown => "unknown".to_owned(),
    }
}

fn minutes(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

/// One team on one line, for `status` without a team.
fn summary_line(name: &TeamName, view: &StatusView) -> String {
    let state = view.state.map_or("unreadable", |s| s.as_str());
    let done = view
        .members
        .iter()
        .filter(|m| m.state.starts_with("done("))
        .count();
    let mut line = format!(
        "{:<12} {:<8} {} members, {done} done",
        name.as_str(),
        state,
        view.members.len()
    );
    if !view.errors.is_empty() {
        line = format!("{line} ({})", view.errors.join("; "));
    }
    line
}

fn render(view: &StatusView) -> String {
    let mut out = Vec::new();
    let name = view.team.as_ref().map_or("?", TeamName::as_str);
    out.push(format!(
        "team {name} \"{}\" {}",
        view.title.as_deref().unwrap_or(""),
        view.state.map_or("unreadable", |s| s.as_str())
    ));
    if let Some(lead) = &view.lead {
        let session = lead.session_id.as_ref().map_or("none", |s| s.as_str());
        out.push(format!(
            "lead: session {session} ({})",
            liveness_text(lead.liveness)
        ));
    }
    out.push("members:".to_owned());
    for m in &view.members {
        let result = m.result.map_or("-", |r| r.as_str());
        out.push(format!(
            "  {:<12} {:<26} gen {}  {} of {}  result {result}  branch {}  {}",
            m.role.as_str(),
            m.state,
            m.generation,
            minutes(m.age_s),
            minutes(m.deadline_s),
            m.branch.as_deref().unwrap_or("-"),
            m.title
        ));
        if let Some(command) = &m.resume_command {
            out.push(format!("    resume: {command}"));
        }
    }
    if !view.messages.is_empty() {
        out.push("last messages:".to_owned());
        for m in &view.messages {
            let to: Vec<&str> = m.to.iter().map(Recipient::as_str).collect();
            out.push(format!(
                "  {} {} {} {} -> {}: {}",
                m.display_id,
                m.time,
                m.kind.as_str(),
                m.from.as_str(),
                to.join(","),
                m.excerpt
            ));
        }
    }
    if !view.stuck_deliveries.is_empty() {
        out.push("deliveries that need attention:".to_owned());
        for d in &view.stuck_deliveries {
            out.push(format!(
                "  {} -> {} {} for {}",
                d.display_id,
                d.to.as_str(),
                d.state.as_str(),
                minutes(d.age_s)
            ));
        }
    }
    if !view.undelivered_results.is_empty() {
        out.push("results the lead has not received:".to_owned());
        for r in &view.undelivered_results {
            out.push(format!(
                "  {} #{} {} {}",
                r.role.as_str(),
                r.n,
                r.report_state.as_str(),
                r.path.display()
            ));
        }
    }
    for e in &view.errors {
        out.push(format!("problem: {e}"));
    }
    out.join("\n") + "\n"
}

#[expect(clippy::print_stdout, reason = "the view is the command's output")]
fn say(text: &str) {
    print!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal on stderr"
)]
fn fail(message: &str, code: u8) -> ExitCode {
    eprintln!("teamwire status: {message}");
    ExitCode::from(code)
}

fn store_failure(team: &TeamName, e: &StoreError) -> ExitCode {
    if let StoreError::Io(message) = e
        && message.starts_with("no database")
    {
        return fail(&format!("no team {team}"), REFUSED);
    }
    fail(&e.to_string(), STORE_ERROR)
}

fn json_text<T: serde::Serialize>(value: &T) -> Result<String, ExitCode> {
    serde_json::to_string_pretty(value)
        .map(|t| t + "\n")
        .map_err(|e| fail(&e.to_string(), STORE_ERROR))
}

pub(crate) fn run(args: &StatusArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let StatusArgs { team, role, json } = args;
    let role = match role.as_deref().map(Role::parse).transpose() {
        Ok(role) => role,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    let home = match home_or_refuse("status", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let Some(team) = team else {
        if role.is_some() {
            return fail("--role needs a team", USAGE);
        }
        return all_teams(&home, *json, wiring);
    };
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    let mut view = match view_of(&home, &team, role.as_ref(), wiring) {
        Ok(view) => view,
        Err(e) => return store_failure(&team, &e),
    };
    hide_lead_from_members(&mut view);
    let text = if *json {
        match json_text(&view) {
            Ok(text) => text,
            Err(code) => return code,
        }
    } else {
        render(&view)
    };
    say(&text);
    let missing_role = role
        .as_ref()
        .is_some_and(|r| !view.members.iter().any(|m| &m.role == r));
    if missing_role {
        ExitCode::from(REFUSED)
    } else {
        ExitCode::SUCCESS
    }
}

/// The lead's session id is the one value a member needs to speak as the
/// lead, so a caller inside a member's session never sees it.
fn hide_lead_from_members(view: &mut StatusView) {
    if carries_member_identity() {
        view.lead = None;
    }
}

fn all_teams(home: &Path, json: bool, wiring: &Wiring) -> ExitCode {
    let mut views = Vec::new();
    for name in team_names(home) {
        let view = view_of(home, &name, None, wiring).unwrap_or_else(|e| StatusView {
            team: Some(name.clone()),
            errors: vec![e.to_string()],
            ..StatusView::default()
        });
        views.push((name, view));
    }
    for (_, view) in &mut views {
        hide_lead_from_members(view);
    }
    if json {
        let only: Vec<&StatusView> = views.iter().map(|(_, v)| v).collect();
        return match json_text(&only) {
            Ok(text) => {
                say(&text);
                ExitCode::SUCCESS
            }
            Err(code) => code,
        };
    }
    if views.is_empty() {
        say("no teams\n");
    }
    for (name, view) in &views {
        say(&(summary_line(name, view) + "\n"));
    }
    ExitCode::SUCCESS
}
