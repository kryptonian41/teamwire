//! `teamwire log`: every message with its delivery state.

use std::process::ExitCode;

use serde_json::json;
use teamwire_core::{Actor, Role, TeamName};
use teamwire_msg::{LogFilter, LogRow, SendError, prove_actor};

use crate::cmd::send::{caller_env, error_code, fail, say, with_read_ctx};
use crate::wiring::Wiring;
use crate::{Global, LogArgs, USAGE};

pub(crate) fn run(args: &LogArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let LogArgs { team, role, json } = args;
    let parsed = TeamName::parse(team)
        .map_err(|e| e.to_string())
        .and_then(|team| {
            let role = role
                .as_deref()
                .map(|r| Role::parse(r).map_err(|e| e.to_string()))
                .transpose()?;
            Ok((team, role))
        });
    let (team, role) = match parsed {
        Ok(parts) => parts,
        Err(e) => return fail("log", &e, USAGE),
    };
    let outcome = with_read_ctx(global, wiring, &team, |ctx| {
        // A member (it holds a token) sees only its own messages. Reading is
        // not speaking: a caller with no token is the person at the
        // terminal, who owns the state directory and sees everything.
        let viewer = if std::env::var_os("TEAMWIRE_TOKEN").is_some() {
            prove_actor(ctx, &caller_env(None)).map_err(SendError::Refused)?
        } else {
            Actor::Lead
        };
        teamwire_msg::log(
            ctx,
            LogFilter {
                viewer,
                role,
                limit: None,
            },
        )
    });
    match outcome {
        Err(code) => code,
        Ok(Err(e)) => fail("log", &e.to_string(), error_code(&e)),
        Ok(Ok(rows)) => {
            if *json {
                say(&format!("{}\n", json!({ "messages": rows })));
            } else if rows.is_empty() {
                say("no messages\n");
            } else {
                for row in &rows {
                    say(&render(row));
                }
            }
            ExitCode::SUCCESS
        }
    }
}

/// `m014 2026-09-29T14:03:11.482Z finding specmsg -> harness: handed_off, rust: undelivered (host_other_session): <excerpt>`
fn render(row: &LogRow) -> String {
    let states: Vec<String> = row
        .deliveries
        .iter()
        .map(|d| {
            let reason = d.reason.map_or(String::new(), |r| format!(" ({r})"));
            let state = serde_json::to_value(d.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            format!("{}: {state}{reason}", d.to)
        })
        .collect();
    format!(
        "{} {} {:<8} {} -> {}: {}\n",
        row.display_id,
        row.time,
        row.kind,
        row.from,
        states.join(", "),
        row.excerpt
    )
}
