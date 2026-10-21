//! `teamwire status`: the read-only team view.

use std::path::Path;
use std::process::ExitCode;

use teamwire_core::{Config, Liveness, Recipient, Role, TeamName};
use teamwire_store::{Ctx, NO_DATABASE_HINT, Store, StoreError};
use teamwire_watch::{ContextView, StatusView, UndeliveredResult, WatchState};

use crate::cmd::send::{REFUSED, STORE, carries_member_identity, fail, home_or_refuse, say};
use crate::wiring::Wiring;
use crate::{Global, StatusArgs, USAGE};

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

/// `72% 720k`, or `-` when the fill is unknown.
fn context_text(context: Option<ContextView>) -> String {
    context.map_or_else(
        || "-".to_owned(),
        |c| format!("{}% {}k", c.pct, c.tokens / 1000),
    )
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

/// The lines that list results the lead has not received. Empty when none.
pub(crate) fn results_block(results: &[UndeliveredResult]) -> Vec<String> {
    if results.is_empty() {
        return Vec::new();
    }
    let mut out = vec!["results the lead has not received:".to_owned()];
    for r in results {
        out.push(format!(
            "  {} #{} {} {}",
            r.role.as_str(),
            r.n,
            r.report_state.as_str(),
            r.path.display()
        ));
    }
    out
}

fn render(view: &StatusView) -> String {
    let mut out = Vec::new();
    let name = view.team.as_ref().map_or("?", TeamName::as_str);
    out.push(format!(
        "team {name} \"{}\" {}",
        view.title.as_deref().unwrap_or(""),
        view.state.map_or("unreadable", |s| s.as_str())
    ));
    if let Some(afk) = &view.afk {
        out.push(if afk.spent {
            format!(
                "afk: budget spent at {}, {} decisions",
                afk.budget_ends_at, afk.decisions
            )
        } else {
            format!(
                "afk: budget ends {}, {} left, {} decisions",
                afk.budget_ends_at,
                minutes(afk.seconds_left),
                afk.decisions
            )
        });
    }
    if let Some(lead) = &view.lead {
        let session = lead.session_id.as_ref().map_or("none", |s| s.as_str());
        out.push(format!(
            "lead: session {session} ({})",
            liveness_text(lead.liveness)
        ));
    }
    out.push(
        match (view.watch.state, view.watch.pid, view.watch.heartbeat_age_s) {
            (WatchState::None, ..) => "watch: none".to_owned(),
            (state, pid, beat) => format!(
                "watch: {} (pid {}, last beat {} ago)",
                if state == WatchState::Running {
                    "running"
                } else {
                    "stale"
                },
                pid.map_or_else(|| "?".to_owned(), |p| p.to_string()),
                beat.map_or_else(|| "?".to_owned(), minutes)
            ),
        },
    );
    out.push("members:".to_owned());
    for m in &view.members {
        let result = m.result.map_or("-", |r| r.as_str());
        out.push(format!(
            "  {:<12} {:<26} gen {}  {} of {}  result {result}  ctx {}  branch {}  {}",
            m.role.as_str(),
            m.state,
            m.generation,
            minutes(m.age_s),
            minutes(m.deadline_s),
            context_text(m.context),
            m.branch.as_deref().unwrap_or("-"),
            m.title
        ));
        if m.older_build {
            out.push(format!(
                "    started by teamwire {}, not this build",
                m.started_by.as_deref().unwrap_or("?")
            ));
        }
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
    if !view.unread_deliveries.is_empty() {
        out.push("deliveries handed off and not read:".to_owned());
        for d in &view.unread_deliveries {
            out.push(format!(
                "  {} -> {} for {}",
                d.display_id,
                d.to.as_str(),
                minutes(d.age_s)
            ));
        }
    }
    if !view.unanswered_questions.is_empty() {
        out.push("questions without an answer:".to_owned());
        for q in &view.unanswered_questions {
            let to: Vec<&str> = q.to.iter().map(Recipient::as_str).collect();
            out.push(format!(
                "  {} from {} to {} for {}",
                q.display_id,
                q.from.as_str(),
                to.join(","),
                minutes(q.age_s)
            ));
        }
    }
    out.extend(results_block(&view.undelivered_results));
    for e in &view.errors {
        out.push(format!("problem: {e}"));
    }
    out.join("\n") + "\n"
}

fn store_failure(team: &TeamName, e: &StoreError) -> ExitCode {
    if matches!(e, StoreError::NoDatabase { .. }) {
        return fail(
            "status",
            &format!("no team {team}. {NO_DATABASE_HINT}"),
            REFUSED,
        );
    }
    fail("status", &e.to_string(), STORE)
}

fn json_text<T: serde::Serialize>(value: &T) -> Result<String, ExitCode> {
    serde_json::to_string_pretty(value)
        .map(|t| t + "\n")
        .map_err(|e| fail("status", &e.to_string(), STORE))
}

pub(crate) fn run(args: &StatusArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let StatusArgs { team, role, json } = args;
    let role = match role.as_deref().map(Role::parse).transpose() {
        Ok(role) => role,
        Err(e) => return fail("status", &e.to_string(), USAGE),
    };
    let home = match home_or_refuse("status", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let Some(team) = team else {
        if role.is_some() {
            return fail("status", "--role needs a team", USAGE);
        }
        return all_teams(&home, *json, wiring);
    };
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("status", &e.to_string(), USAGE),
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
    if unreadable(&view) {
        return ExitCode::from(STORE);
    }
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

/// `status_with` turns a failed read into a view with the errors and no
/// state; that is a store error, not a team to describe.
fn unreadable(view: &StatusView) -> bool {
    view.state.is_none() && !view.errors.is_empty()
}

fn all_teams(home: &Path, json: bool, wiring: &Wiring) -> ExitCode {
    let names = match teamwire_store::team_names(home) {
        Ok(names) => names,
        Err(e) => return fail("status", &e.to_string(), STORE),
    };
    let mut views = Vec::new();
    for name in names {
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
    let code = if views.iter().any(|(_, view)| unreadable(view)) {
        ExitCode::from(STORE)
    } else {
        ExitCode::SUCCESS
    };
    if json {
        let only: Vec<&StatusView> = views.iter().map(|(_, v)| v).collect();
        return match json_text(&only) {
            Ok(text) => {
                say(&text);
                code
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
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_shows_the_percent_and_thousands_of_tokens_or_a_dash() {
        let c = ContextView {
            tokens: 720_091,
            window: 1_000_000,
            pct: 72,
            handoff_at: 400_000,
        };
        assert_eq!(context_text(Some(c)), "72% 720k");
        assert_eq!(context_text(None), "-");
    }
}
