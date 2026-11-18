//! `teamwire dialog show`: a member's recorded dialogs, the whole call each
//! asks for, and the member's screen. Reads only: the store is opened
//! read-only, and the host is only asked for its state and its screen.
//!
//! Every text that comes from the member's model or screen goes through
//! `shown`, so a hidden or reordering character prints as `\u{...}`.

use std::process::ExitCode;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use teamwire_core::{
    DialogKind, DialogState, DialogVerdict, Harness, Member, PermissionDecision, Role, TeamName,
    Timestamp, clock_time, shown,
};
use teamwire_store::{Ctx, DialogRecord, Party};

use crate::cmd::shared::{Opening, REFUSED, STORE, fail, refuse_member, say, with_ctx_using};
use crate::wiring::Wiring;
use crate::{DialogShowArgs, Global, USAGE};

const COMMAND: &str = "dialog show";
/// Each host call is bounded, so a hung pane cannot hang the lead.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const SCREEN_LINES: u16 = 60;
/// The newest dialogs shown.
const DIALOGS_SHOWN: usize = 5;

#[derive(Serialize)]
struct View {
    team: String,
    role: String,
    harness: String,
    host: String,
    state: Option<String>,
    kind: Option<DialogKind>,
    dialogs: Vec<DialogView>,
    cannot_answer: Option<String>,
    screen: Option<String>,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct DialogView {
    id: String,
    state: DialogState,
    tool: String,
    summary: String,
    input: Option<Value>,
    why_not: Option<String>,
    subagent: bool,
    opened_at: String,
    expires_at: String,
    seconds_left: Option<u64>,
    verdict: Option<DialogVerdict>,
    reason: Option<String>,
    answered_at: Option<String>,
    closed_at: Option<String>,
}

pub(super) fn run(args: &DialogShowArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let DialogShowArgs { team, role, json } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(COMMAND, &e.to_string(), USAGE),
    };
    let role = match Role::parse(role) {
        Ok(role) => role,
        Err(e) => return fail(COMMAND, &e.to_string(), USAGE),
    };
    if let Some(code) = refuse_member(COMMAND) {
        return code;
    }
    let opening = Opening {
        command: COMMAND,
        read_only: true,
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "dialog_show", opening, |ctx| {
        show(ctx, &team, &role, *json)
    })
    .unwrap_or_else(|code| code)
}

fn show(ctx: &Ctx<'_>, team: &TeamName, role: &Role, json: bool) -> ExitCode {
    let now = ctx.clock.now();
    let read = ctx
        .store
        .read(|tx| Ok((tx.member(role)?, tx.dialogs_of_role(role, now)?)));
    let (member, mut dialogs) = match read {
        Ok(rows) => rows,
        Err(e) => return fail(COMMAND, &format!("the team cannot be read: {e}"), STORE),
    };
    let Some(member) = member else {
        return fail(
            COMMAND,
            &format!("no member {role} in team {team}"),
            REFUSED,
        );
    };
    dialogs.retain(|d| d.generation == member.generation);
    dialogs.truncate(DIALOGS_SHOWN);
    let mut errors = Vec::new();
    let (state, screen, kind, hooked) = look(ctx, &member, &mut errors);
    let pending = dialogs.iter().any(|d| d.state == DialogState::Pending);
    let cannot_answer = (!pending && kind == Some(DialogKind::Permission))
        .then(|| why_cannot_answer(&member, hooked, dialogs.first()));
    let view = View {
        team: team.to_string(),
        role: role.to_string(),
        harness: member.harness.to_string(),
        host: member.host.to_string(),
        state,
        kind,
        dialogs: dialogs.iter().map(|d| dialog_view(d, now)).collect(),
        cannot_answer,
        screen,
        errors,
    };
    if json {
        match serde_json::to_string_pretty(&view) {
            Ok(text) => say(&format!("{text}\n")),
            Err(e) => return fail(COMMAND, &e.to_string(), STORE),
        }
    } else {
        say(&text(&view, &dialogs, now));
    }
    ExitCode::SUCCESS
}

/// The host's word for the place, the screen tail, the dialog kind the
/// harness reads in it, and whether the harness has a permission hook. A
/// failed call adds one line to `errors` and leaves its field empty.
fn look(
    ctx: &Ctx<'_>,
    member: &Member,
    errors: &mut Vec<String>,
) -> (Option<String>, Option<String>, Option<DialogKind>, bool) {
    let (harness, host) = match ctx.adapters(&Party::Member(member.role.clone())) {
        Ok(adapters) => adapters,
        Err(e) => {
            errors.push(format!("the member's harness or host: {e}"));
            return (None, None, None, false);
        }
    };
    let hooked = has_hook(harness);
    let Some(place) = &member.place else {
        errors.push("the member has no pane recorded".to_owned());
        return (None, None, None, hooked);
    };
    let state = if host.capabilities().reports_state {
        match host.state(place, CALL_TIMEOUT) {
            Ok(state) => Some(state.as_str().to_owned()),
            Err(e) => {
                errors.push(format!("the host's state: {e}"));
                None
            }
        }
    } else {
        None
    };
    match host.read_tail(place, SCREEN_LINES, CALL_TIMEOUT) {
        Ok(tail) => {
            let kind = harness.dialog_kind(&tail);
            (state, Some(tail), Some(kind), hooked)
        }
        Err(e) => {
            errors.push(format!("the screen: {e}"));
            (state, None, None, hooked)
        }
    }
}

/// Whether the harness can print a permission reply, so has a hook to take
/// an answer. The bytes are not used.
fn has_hook(harness: &dyn Harness) -> bool {
    let probe = PermissionDecision {
        verdict: DialogVerdict::Approve,
        message: None,
    };
    harness.permission_reply(&probe).is_ok()
}

/// Why nobody can answer the permission dialog on screen, the first reason
/// that holds, in a fixed order.
fn why_cannot_answer(member: &Member, hooked: bool, newest: Option<&DialogRecord>) -> String {
    if !hooked {
        return format!(
            "the member's harness {} has no permission hook",
            member.harness
        );
    }
    let Some(newest) = newest else {
        return "no permission hook recorded this dialog; the member may have started before \
                teamwire had the hook"
            .to_owned();
    };
    match newest.state {
        DialogState::Expired => format!(
            "the member's permission hook stopped waiting at {}; Claude Code denies the call on \
             its own, or, for a dialog with no countdown, it waits for a person in the pane",
            clock_time(newest.expires_at)
        ),
        DialogState::Unanswerable => shown(newest.request.why_not.as_deref().unwrap_or_default()),
        DialogState::Answered | DialogState::Taken | DialogState::Pending => format!(
            "dialog {} was answered at {}; the screen may not have redrawn yet, so run dialog \
             show again",
            newest.id,
            newest.answered_at.map_or_else(String::new, clock_time)
        ),
    }
}

fn dialog_view(d: &DialogRecord, now: Timestamp) -> DialogView {
    DialogView {
        id: d.id.to_string(),
        state: d.state,
        tool: shown(&d.request.tool),
        summary: shown(&d.request.summary),
        input: d.request.input.clone(),
        why_not: d.request.why_not.clone(),
        subagent: d.request.subagent,
        opened_at: d.opened_at.to_string(),
        expires_at: d.expires_at.to_string(),
        seconds_left: (d.state == DialogState::Pending)
            .then(|| d.expires_at.saturating_since(now).as_secs()),
        verdict: d.answer.as_ref().map(|a| a.verdict),
        reason: d.answer.as_ref().map(|a| a.reason.clone()),
        answered_at: d.answered_at.map(|t| t.to_string()),
        closed_at: d.closed_at.map(|t| t.to_string()),
    }
}

/// The first line's ending for each kind of screen.
fn place_text(kind: Option<DialogKind>) -> &'static str {
    match kind {
        Some(DialogKind::Permission) => "at a permission dialog",
        Some(DialogKind::Trust) => "at a trust dialog",
        Some(DialogKind::Login) => "at a login dialog",
        Some(DialogKind::Update) => "at an update dialog",
        Some(DialogKind::Unknown) => "at a dialog teamwire cannot name",
        Some(DialogKind::NoDialog) => "at no dialog",
        Some(DialogKind::CannotTell) | None => "teamwire cannot tell whether it shows a dialog",
    }
}

fn text(view: &View, dialogs: &[DialogRecord], now: Timestamp) -> String {
    let mut out = format!(
        "{} ({} on {}): {}\n",
        view.role,
        view.harness,
        view.host,
        place_text(view.kind)
    );
    if let Some(why) = &view.cannot_answer {
        line(
            &mut out,
            &format!("teamwire cannot answer this dialog: {why}"),
        );
    }
    let pending = dialogs.iter().any(|d| d.state == DialogState::Pending);
    // A screen the harness read as something other than a permission
    // dialog, while one is pending: someone may have answered it in the pane.
    let elsewhere = matches!(
        view.kind,
        Some(DialogKind::NoDialog | DialogKind::Trust | DialogKind::Login | DialogKind::Update)
    );
    if pending && elsewhere {
        out.push_str(
            "warning: the screen shows no permission dialog now; someone may have answered it in the pane\n",
        );
    }
    for d in dialogs {
        out.push_str(&dialog_text(view, d, now));
    }
    for e in &view.errors {
        line(&mut out, &format!("error: {e}"));
    }
    if let Some(screen) = &view.screen {
        line(&mut out, &format!("screen (last {SCREEN_LINES} lines):"));
        for text in shown(screen).lines() {
            line(&mut out, &format!("  {text}"));
        }
    }
    out
}

fn dialog_text(view: &View, d: &DialogRecord, now: Timestamp) -> String {
    let id = &d.id;
    let at = |t: Option<Timestamp>| t.map_or_else(String::new, clock_time);
    let verdict = d.answer.as_ref().map_or("", |a| a.verdict.as_str());
    match d.state {
        DialogState::Answered => format!(
            "dialog {id}: answered {verdict} at {}, waiting for the member's hook\n",
            at(d.answered_at)
        ),
        DialogState::Taken => format!(
            "dialog {id}: {verdict} by the lead at {}: {}\n",
            at(d.answered_at),
            shown(d.answer.as_ref().map_or("", |a| a.reason.as_str()))
        ),
        DialogState::Expired => format!(
            "dialog {id}: expired at {}; nobody answered\n",
            clock_time(d.expires_at)
        ),
        DialogState::Unanswerable => format!(
            "dialog {id}: teamwire cannot answer it: {}\n",
            shown(d.request.why_not.as_deref().unwrap_or_default())
        ),
        DialogState::Pending => pending_text(view, d, now),
    }
}

/// The whole call of a pending dialog, and the command that answers it.
fn pending_text(view: &View, d: &DialogRecord, now: Timestamp) -> String {
    let id = &d.id;
    let mut out = format!(
        "dialog {id}: pending, answer before {} ({} s left)\n  tool: {}\n",
        clock_time(d.expires_at),
        d.expires_at.saturating_since(now).as_secs(),
        shown(&d.request.tool)
    );
    let key = match d.request.tool.as_str() {
        "Bash" => Some("command"),
        "Write" | "Edit" => Some("file_path"),
        "WebFetch" => Some("url"),
        _ => None,
    };
    let whole = key.and_then(|k| d.request.input.as_ref()?.get(k)?.as_str());
    if let Some(whole) = whole {
        out.push_str("  command:\n");
        for text in shown(whole).lines() {
            line(&mut out, &format!("    {text}"));
        }
    }
    if let Some(input) = &d.request.input {
        line(&mut out, &format!("  input: {}", shown(&input.to_string())));
    }
    line(
        &mut out,
        &format!(
            "  answer: teamwire dialog answer {} --role {} --id {id} --approve|--deny --reason TEXT",
            view.team, view.role
        ),
    );
    out
}

/// Adds `text` and a newline to `out`.
fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}
