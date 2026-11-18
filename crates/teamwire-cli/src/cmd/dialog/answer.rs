//! `teamwire dialog answer`: the lead's answer to one permission dialog.
//! The dialog id is the proof: the answer is recorded only while that id is
//! pending for the member's current session, in one transaction (ADR 0052).
//! Then the command waits for the member's hook to take it.

use std::process::ExitCode;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    ANSWER_TAKE_WAIT, Actor, DialogState, DialogVerdict, MemberState, PermissionAnswer,
    PermissionDecision, Role, TeamName, TeamState, Ulid,
};
use teamwire_msg::prove_actor_logged;
use teamwire_store::{AnswerOutcome, Ctx, DialogRecord};

use crate::cmd::shared::{
    Opening, REFUSED, STORE, caller_env, fail, refuse_member, say, warn, with_ctx_using,
};
use crate::wiring::Wiring;
use crate::{DialogAnswerArgs, Global, USAGE};

/// The answer is recorded, but the member's hook did not take it in time.
const NOT_TAKEN: u8 = 7;

const COMMAND: &str = "dialog answer";

/// How often the command reads the dialog while it waits for the hook.
const POLL: Duration = Duration::from_millis(100);

pub(super) fn run(args: &DialogAnswerArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let DialogAnswerArgs {
        team,
        role,
        id,
        approve,
        reason,
        json,
        ..
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(COMMAND, &e.to_string(), USAGE),
    };
    let role = match Role::parse(role) {
        Ok(role) => role,
        Err(e) => return fail(COMMAND, &e.to_string(), USAGE),
    };
    let id = match Ulid::parse(id) {
        Ok(id) => id,
        Err(e) => return fail(COMMAND, &format!("--id: {e}"), USAGE),
    };
    if reason.trim().is_empty() {
        return fail(COMMAND, "--reason is empty", USAGE);
    }
    if let Some(code) = refuse_member(COMMAND) {
        return code;
    }
    let answer = PermissionAnswer {
        verdict: if *approve {
            DialogVerdict::Approve
        } else {
            DialogVerdict::Deny
        },
        reason: reason.trim().to_owned(),
    };
    let asked = Asked {
        team: &team,
        role: &role,
        id: &id,
        answer: &answer,
        json: *json,
    };
    let opening = Opening {
        command: COMMAND,
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "dialog_answer", opening, |ctx| {
        answer_in(ctx, &asked)
    })
    .unwrap_or_else(|code| code)
}

struct Asked<'a> {
    team: &'a TeamName,
    role: &'a Role,
    id: &'a Ulid,
    answer: &'a PermissionAnswer,
    json: bool,
}

/// `teamwire dialog answer: refused.<code>: <text>`, exit 1.
fn refused(code: &str, text: &str) -> ExitCode {
    fail(COMMAND, &format!("refused.{code}: {text}"), REFUSED)
}

fn answer_in(ctx: &mut Ctx<'_>, asked: &Asked<'_>) -> ExitCode {
    let Asked { team, role, id, .. } = *asked;
    match prove_actor_logged(ctx, &caller_env(Some("lead".into()))) {
        Ok(Actor::Lead) => {}
        Ok(_) => {
            return refused(
                "identity",
                "only the team's lead session may answer a dialog",
            );
        }
        Err(refusal) => return refused("identity", &refusal.to_string()),
    }
    let rows = ctx.store.read(|tx| Ok((tx.team()?, tx.member(role)?)));
    let (team_row, member) = match rows {
        Ok(rows) => rows,
        Err(e) => return fail(COMMAND, &format!("the team cannot be read: {e}"), STORE),
    };
    if team_row.is_none_or(|t| t.state != TeamState::Open) {
        return refused("team", &format!("team {team} is not open"));
    }
    let Some(member) = member.filter(|m| m.state == MemberState::Started) else {
        return refused("role", &format!("{role} is not a started member of {team}"));
    };
    // Only a harness that can print a reply has a permission hook to take
    // the answer. The bytes themselves are not used.
    let probe = PermissionDecision {
        verdict: DialogVerdict::Approve,
        message: None,
    };
    let hooked = ctx
        .registry
        .harness(&member.harness)
        .is_ok_and(|h| h.permission_reply(&probe).is_ok());
    if !hooked {
        return refused(
            "harness",
            &format!("harness {} cannot answer a dialog", member.harness),
        );
    }
    let now = ctx.clock.now();
    let outcome = ctx
        .store
        .write(|tx| tx.answer_dialog(now, role, id, asked.answer));
    match outcome {
        Ok(AnswerOutcome::Recorded(_)) => {}
        Ok(AnswerOutcome::NoDialog) => {
            return refused("no_dialog", &format!("no dialog {id} for {role}"));
        }
        Ok(AnswerOutcome::Changed) => {
            return refused(
                "changed",
                &format!("dialog {id} belongs to an earlier session of {role}"),
            );
        }
        Ok(AnswerOutcome::NotPending(state)) => return not_pending(ctx, asked, state),
        Err(e) => return fail(COMMAND, &format!("the answer was not recorded: {e}"), STORE),
    }
    let taken = match wait_for_take(ctx, id) {
        Ok(taken) => taken,
        Err(e) => {
            return fail(
                COMMAND,
                &format!("{e}; the answer may be recorded; run teamwire dialog show"),
                STORE,
            );
        }
    };
    report(asked, taken);
    if taken {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(NOT_TAKEN)
    }
}

/// Whether the member's hook took the answer within `ANSWER_TAKE_WAIT`.
fn wait_for_take(ctx: &Ctx<'_>, id: &Ulid) -> Result<bool, teamwire_store::StoreError> {
    let deadline = ctx.clock.monotonic() + ANSWER_TAKE_WAIT;
    loop {
        let state = ctx
            .store
            .read(|tx| tx.dialog(id, ctx.clock.now()))?
            .map(|d| d.state);
        if state == Some(DialogState::Taken) {
            return Ok(true);
        }
        if ctx.clock.monotonic() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(POLL);
    }
}

/// The refusal for a dialog that is no longer pending, naming what it is.
fn not_pending(ctx: &Ctx<'_>, asked: &Asked<'_>, state: DialogState) -> ExitCode {
    let id = asked.id;
    let record: Option<DialogRecord> = ctx
        .store
        .read(|tx| tx.dialog(id, ctx.clock.now()))
        .ok()
        .flatten();
    match state {
        DialogState::Answered | DialogState::Taken => {
            let verdict = record
                .and_then(|r| r.answer)
                .map_or("an answer", |a| match a.verdict {
                    DialogVerdict::Approve => "approve",
                    DialogVerdict::Deny => "deny",
                });
            refused(
                "answered",
                &format!("dialog {id} was already answered: {verdict}"),
            )
        }
        DialogState::Expired => {
            let at =
                record.map_or_else(|| "its time limit".to_owned(), |r| r.expires_at.to_string());
            refused(
                "expired",
                &format!(
                    "dialog {id} expired at {at}; the member's hook gave up, and Claude Code \
                     denies the call on its own. To tell the member not to retry it, send a note \
                     with teamwire send"
                ),
            )
        }
        DialogState::Unanswerable | DialogState::Pending => {
            let why = record
                .and_then(|r| r.request.why_not)
                .unwrap_or_else(|| "teamwire does not answer this call".to_owned());
            refused("not_answerable", &why)
        }
    }
}

fn report(asked: &Asked<'_>, taken: bool) {
    let Asked {
        team,
        role,
        id,
        answer,
        json,
    } = *asked;
    let verdict = answer.verdict.as_str();
    if json {
        let doc = json!({
            "team": team.as_str(), "role": role.as_str(), "id": id.to_string(),
            "verdict": verdict, "reason": answer.reason, "taken": taken,
        });
        say(&format!("{doc}\n"));
    } else if taken {
        say(&format!(
            "answered dialog {id} of {role} in team {team}: {verdict} (taken by the member's permission hook)\n"
        ));
    } else {
        say(&format!(
            "answered dialog {id} of {role} in team {team}: {verdict} (recorded, not taken)\n"
        ));
    }
    if !taken {
        warn(&format!(
            "not taken: the member's permission hook did not take the answer within 5 s; the \
             dialog may be closed already, or Claude Code denies it on its own at its time \
             limit. Run teamwire dialog show {team} --role {role}."
        ));
    }
}
