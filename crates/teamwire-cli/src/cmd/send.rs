//! `teamwire send`: send one message. Also holds what the message commands
//! (`resend`, `log`, `inbox`, `ack`, `members`, `lead`) share: opening the
//! team's store, reading who the caller says it is, and printing.

use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_core::{Config, DisplayId, MessageType, Recipient, SendRequest, TeamName};
use teamwire_msg::{
    CallerEnv, RecipientState, SendError, SendReport, parse_type_logged, prove_actor_logged,
    send_with, watch_running,
};
use teamwire_store::{Ctx, Store, StoreError};

use crate::wiring::Wiring;
use crate::{Global, SendArgs, USAGE};

/// Refused by a rule; nothing changed (docs/api/cli.md).
pub(crate) const REFUSED: u8 = 1;
/// At least one recipient is `undelivered`.
const NOT_DELIVERED: u8 = 4;
/// At least one recipient is `unconfirmed`, none `undelivered`.
const UNCONFIRMED: u8 = 5;
/// The store could not be read or written.
pub(crate) const STORE: u8 = 6;

pub(crate) fn run(args: &SendArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let SendArgs {
        team,
        to,
        kind,
        text,
        file,
        commit,
        re,
        key,
        from,
    } = args;
    let Some(body) = read_body(text.as_deref(), file.as_deref()) else {
        return fail("send", "the text could not be read from --file", USAGE);
    };
    let parsed = TeamName::parse(team)
        .map_err(|e| e.to_string())
        .and_then(|team| {
            let to = to
                .iter()
                .map(|r| Recipient::parse(r).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let re = re
                .as_deref()
                .map(|id| DisplayId::parse(id).map_err(|e| e.to_string()))
                .transpose()?;
            Ok((team, to, re))
        });
    let (team, to, re) = match parsed {
        Ok(parts) => parts,
        Err(e) => return fail("send", &e, USAGE),
    };
    let outcome = with_ctx(global, wiring, &team, |ctx| {
        let env = caller_env(from.clone());
        let actor = match prove_actor_logged(ctx, &env) {
            Ok(actor) => actor,
            Err(refusal) => return Outcome::Failed(SendError::Refused(refusal)),
        };
        let Ok(kind) = parse_type_logged(ctx, &actor, kind, &to) else {
            return Outcome::Unknown(format!("unknown type {kind:?}"));
        };
        let request = SendRequest {
            team: team.clone(),
            actor,
            to,
            kind,
            body,
            commit: commit.clone(),
            re,
            client_key: key.clone(),
        };
        // The id and key go out before any host call, so a sender killed in
        // the middle of a delivery can still name its message (ADR 0003).
        let sent = send_with(ctx, request, &mut |stored| {
            let verb = if stored.duplicate {
                "resuming"
            } else {
                "stored"
            };
            warn(&format!(
                "{verb} {} (key {}); if this command is interrupted, run: teamwire send {team} ... --key {}, or teamwire resend {team} {}",
                stored.display_id, stored.key, stored.key, stored.display_id
            ));
        });
        match sent {
            Ok(report) => {
                let watching = (kind == MessageType::Reprompt).then(|| {
                    watch_running(ctx)
                        .ok()
                        .flatten()
                        .map_or(Watch::NotRunning, Watch::Running)
                });
                Outcome::Sent(Box::new(report), watching)
            }
            Err(e) => Outcome::Failed(e),
        }
    });
    match outcome {
        Err(code) => code,
        Ok(Outcome::Unknown(why)) => {
            fail("send", &format!("refused.type_not_allowed: {why}"), REFUSED)
        }
        Ok(Outcome::Failed(e)) => fail("send", &e.to_string(), error_code(&e)),
        Ok(Outcome::Sent(report, watching)) => {
            print_report(&report);
            if let Some(watching) = watching {
                match watching {
                    Watch::Running(pid) => say(&format!("a watch is running (pid {pid})\n")),
                    Watch::NotRunning => say(&format!(
                        "no watch is running: start one with `teamwire watch {team}`\n"
                    )),
                }
            }
            ExitCode::from(exit_of(&report))
        }
    }
}

/// Whether a watch is running, told to the lead after a reprompt.
enum Watch {
    Running(teamwire_core::Pid),
    NotRunning,
}

enum Outcome {
    Sent(Box<SendReport>, Option<Watch>),
    Failed(SendError),
    Unknown(String),
}

fn read_body(text: Option<&str>, file: Option<&std::path::Path>) -> Option<String> {
    match (text, file) {
        (Some(text), _) => Some(text.to_owned()),
        (None, Some(path)) => std::fs::read_to_string(path).ok(),
        (None, None) => None,
    }
}

/// One line per recipient on stdout. A recipient that was not reached also
/// gets its reason on stderr, the line a script or an agent looks for.
pub(crate) fn print_report(report: &SendReport) {
    say(&format!("{}\n", report.display_id));
    for d in &report.deliveries {
        let reason = d.reason.map_or(String::new(), |r| format!(" ({r})"));
        say(&format!("  {}: {}{reason}\n", d.to, state_word(d.state)));
        match d.state {
            RecipientState::Undelivered => warn(&format!(
                "NOT DELIVERED to {}: {}",
                d.to,
                d.reason.map_or("unknown", |r| r.as_str())
            )),
            RecipientState::Unconfirmed => warn(&format!(
                "UNCONFIRMED to {}: {}. The line may or may not be on its screen; it was not sent again. `teamwire resend` sends it again.",
                d.to,
                d.reason.map_or("unknown", |r| r.as_str())
            )),
            RecipientState::Pending
            | RecipientState::HandedOff
            | RecipientState::Acked
            | RecipientState::Skipped => {}
        }
    }
}

fn state_word(state: RecipientState) -> &'static str {
    match state {
        RecipientState::Pending => "pending",
        RecipientState::HandedOff => "handed_off",
        RecipientState::Unconfirmed => "unconfirmed",
        RecipientState::Undelivered => "undelivered",
        RecipientState::Acked => "acked",
        RecipientState::Skipped => "skipped",
    }
}

/// 4 when any recipient is undelivered, else 5 when any is unconfirmed.
pub(crate) fn exit_of(report: &SendReport) -> u8 {
    let any = |s| report.deliveries.iter().any(|d| d.state == s);
    if any(RecipientState::Undelivered) {
        NOT_DELIVERED
    } else if any(RecipientState::Unconfirmed) {
        UNCONFIRMED
    } else {
        0
    }
}

pub(crate) fn error_code(e: &SendError) -> u8 {
    match e {
        SendError::Refused(_) | SendError::NotResendable(_) | SendError::Internal(_) => REFUSED,
        SendError::InvalidArgument(_) => USAGE,
        SendError::Store(_) => STORE,
    }
}

/// What the process says about itself: the member's variables and the
/// claimed sender (ADR 0008).
pub(crate) fn caller_env(from: Option<String>) -> CallerEnv {
    CallerEnv {
        team: std::env::var("TEAMWIRE_TEAM").ok(),
        role: std::env::var("TEAMWIRE_ROLE").ok(),
        token: std::env::var("TEAMWIRE_TOKEN").ok(),
        from,
    }
}

/// `--home`, else `$TEAMWIRE_HOME`, else `~/.teamwire`.
fn state_home(global: &Global) -> PathBuf {
    let Global { home } = global;
    home.clone()
        .or_else(|| std::env::var_os("TEAMWIRE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".teamwire")))
        .unwrap_or_else(|| PathBuf::from(".teamwire"))
}

/// Opens the team's store and runs `f` with a `Ctx` on it. A store that
/// cannot be opened is reported and turned into the exit code to return.
pub(crate) fn with_ctx<T>(
    global: &Global,
    wiring: &Wiring,
    team: &TeamName,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, ExitCode> {
    let home = state_home(global);
    let mut store = match Store::open_existing(&home, team) {
        Ok(store) => store,
        Err(StoreError::Io(message)) if message.starts_with("no database") => {
            return Err(fail("", &message, REFUSED));
        }
        Err(e) => return Err(fail("", &e.to_string(), STORE)),
    };
    let config = Config::default();
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    Ok(f(&mut ctx))
}

/// `with_ctx` for a command that only reads (`log`, `members`): the store is
/// opened read-only, never created or migrated, and a database of another
/// schema version is refused (WAT-11). A read never writes, not even a
/// refusal.
pub(crate) fn with_read_ctx<T>(
    global: &Global,
    wiring: &Wiring,
    team: &TeamName,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, ExitCode> {
    let home = state_home(global);
    if !home
        .join("teams")
        .join(team.as_str())
        .join("team.db")
        .is_file()
    {
        return Err(fail(
            "",
            &format!("no database for team {team} under {}", home.display()),
            REFUSED,
        ));
    }
    let mut store = match Store::open_read_only(&home, team) {
        Ok(store) => store,
        Err(e) => return Err(fail("", &e.to_string(), STORE)),
    };
    let config = Config::default();
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    Ok(f(&mut ctx))
}

#[expect(clippy::print_stdout, reason = "the command's result is its output")]
pub(crate) fn say(text: &str) {
    print!("{text}");
}

#[expect(clippy::print_stderr, reason = "the binary reports on stderr")]
pub(crate) fn warn(text: &str) {
    eprintln!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal on stderr"
)]
pub(crate) fn fail(command: &str, message: &str, code: u8) -> ExitCode {
    if command.is_empty() {
        eprintln!("teamwire: {message}");
    } else {
        eprintln!("teamwire {command}: {message}");
    }
    ExitCode::from(code)
}
