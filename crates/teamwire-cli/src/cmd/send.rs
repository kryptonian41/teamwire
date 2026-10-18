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
use teamwire_store::{Ctx, NO_DATABASE_HINT, Party, Store, StoreError};

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
        // A member's `--from` defaults to its role. With no role and no
        // token the caller claims to be the lead, as `resend` does, and the
        // harness session decides.
        let from = from
            .clone()
            .or_else(|| member_token().is_none().then(|| "lead".to_owned()));
        let env = caller_env(from);
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
                    let reads = ctx
                        .adapters(&Party::Lead)
                        .map_or(true, |(h, _)| h.capabilities().reads_watch_lines);
                    (watch_state(watch_running(ctx)), reads)
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
            print_report(&team, &report);
            if let Some((watching, reads)) = watching {
                say(&watch_line(&team, &watching, reads));
            }
            ExitCode::from(exit_of(&report))
        }
    }
}

/// Whether a watch is running, told to the lead after a reprompt.
enum Watch {
    Running(teamwire_core::Pid),
    NotRunning,
    /// The lease could not be read; carries the error text.
    Unknown(String),
}

/// What a failed lease read says: the lead is not told "no watch" about
/// something the code could not read.
fn watch_state(lease: Result<Option<teamwire_core::Pid>, SendError>) -> Watch {
    match lease {
        Ok(Some(pid)) => Watch::Running(pid),
        Ok(None) => Watch::NotRunning,
        Err(e) => Watch::Unknown(e.to_string()),
    }
}

/// `reads_watch_lines` is the lead's harness capability of that name: a lead
/// without it runs the watch in the background (ADR 0023).
fn watch_line(team: &TeamName, watching: &Watch, reads_watch_lines: bool) -> String {
    match watching {
        Watch::Running(pid) => format!("a watch is running (pid {pid})\n"),
        Watch::NotRunning => {
            let command = if reads_watch_lines {
                format!("teamwire watch {team}")
            } else {
                format!("nohup teamwire watch {team} --deliver >/dev/null 2>&1 &")
            };
            format!("no watch is running: start one with `{command}`\n")
        }
        Watch::Unknown(why) => format!("could not tell whether a watch is running: {why}\n"),
    }
}

enum Outcome {
    Sent(Box<SendReport>, Option<(Watch, bool)>),
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
pub(crate) fn print_report(team: &TeamName, report: &SendReport) {
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
            RecipientState::Pending => warn(&pending_line(team, &report.display_id, &d.to)),
            RecipientState::HandedOff | RecipientState::Acked | RecipientState::Skipped => {}
        }
    }
}

/// A pending recipient after the command ends means its delivery record
/// could not be written; the exit code is 6 and this says why.
fn pending_line(team: &TeamName, id: &DisplayId, to: &Recipient) -> String {
    format!(
        "NOT RECORDED for {to}: the delivery record could not be written (store error); run teamwire resend {team} {id}"
    )
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

/// 6 when any recipient is still pending, because its delivery record could
/// not be written; else 4 when any is undelivered, else 5 when any is
/// unconfirmed.
pub(crate) fn exit_of(report: &SendReport) -> u8 {
    let any = |s| report.deliveries.iter().any(|d| d.state == s);
    if any(RecipientState::Pending) {
        STORE
    } else if any(RecipientState::Undelivered) {
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
        token: member_token(),
        from,
        hints: Vec::new(),
    }
}

/// The member token the process carries. Every command reads it here, so
/// they agree on what counts as having one: the variable, else the file
/// `TEAMWIRE_TOKEN_FILE` names (ADR 0023 point 4).
pub(crate) fn member_token() -> Option<String> {
    teamwire_msg::caller_token(|name| std::env::var(name).ok())
}

/// Whether the process runs inside a member's session. The rule is
/// `CallerEnv::is_member_session`, the one the message crate's proofs use, so
/// the CLI and the proofs can never disagree about who is a member.
pub(crate) fn carries_member_identity() -> bool {
    caller_env(None).is_member_session()
}

/// A lead-only command (`close`, `watch`, `launch --add`) run from a member's
/// session is refused.
pub(crate) fn refuse_member(command: &str) -> Option<ExitCode> {
    carries_member_identity().then(|| {
        fail(
            command,
            "refused.identity: this command is for the lead; a member session cannot run it",
            REFUSED,
        )
    })
}

/// `--home`, else `$TEAMWIRE_HOME`, else `~/.teamwire`, made absolute. A
/// relative path is handed on to members, whose working directory is not
/// this one, so it must not stay relative. None when nothing names a
/// directory.
pub(crate) fn state_home(global: &Global) -> Option<PathBuf> {
    let Global { home } = global;
    let chosen = home
        .clone()
        .or_else(|| std::env::var_os("TEAMWIRE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".teamwire")))?;
    std::path::absolute(chosen).ok()
}

/// `state_home`, or the refusal to print and exit with when nothing names
/// the state directory.
pub(crate) fn home_or_refuse(command: &str, global: &Global) -> Result<PathBuf, ExitCode> {
    state_home(global).ok_or_else(|| {
        fail(
            command,
            "no state directory: set --home, TEAMWIRE_HOME or HOME",
            REFUSED,
        )
    })
}

/// The refusal text for a team whose database file is missing.
pub(crate) fn no_database_text(team: &TeamName, path: &std::path::Path) -> String {
    format!(
        "no database for team {team} at {}. {NO_DATABASE_HINT}",
        path.display()
    )
}

/// Opens the team's store and runs `f` with a `Ctx` on it. A store that
/// cannot be opened is reported and turned into the exit code to return.
pub(crate) fn with_ctx<T>(
    global: &Global,
    wiring: &Wiring,
    team: &TeamName,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, ExitCode> {
    let home = home_or_refuse("", global)?;
    let mut store = match Store::open_existing(&home, team) {
        Ok(store) => store,
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(team, &path);
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
    let home = home_or_refuse("", global)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> TeamName {
        TeamName::parse("alpha").unwrap()
    }

    fn report(states: &[RecipientState]) -> SendReport {
        SendReport {
            id: teamwire_core::Ulid::parse("01M466HBGM3C8KG2XPMZWRQCH4").unwrap(),
            display_id: DisplayId::parse("m001").unwrap(),
            seq: 1,
            deliveries: states
                .iter()
                .map(|state| teamwire_msg::RecipientReport {
                    to: Recipient::parse("api").unwrap(),
                    state: *state,
                    reason: None,
                    attempts: None,
                })
                .collect(),
            duplicate_of_key: false,
        }
    }

    #[test]
    fn a_pending_recipient_is_explained() {
        let line = pending_line(
            &team(),
            &DisplayId::parse("m007").unwrap(),
            &Recipient::parse("api").unwrap(),
        );
        assert_eq!(
            line,
            "NOT RECORDED for api: the delivery record could not be written (store error); run teamwire resend alpha m007"
        );
    }

    #[test]
    fn a_pending_recipient_exits_6() {
        assert_eq!(exit_of(&report(&[RecipientState::Pending])), STORE);
        assert_eq!(
            exit_of(&report(&[
                RecipientState::HandedOff,
                RecipientState::Pending
            ])),
            STORE
        );
        assert_eq!(
            exit_of(&report(&[RecipientState::Undelivered])),
            NOT_DELIVERED
        );
        assert_eq!(exit_of(&report(&[RecipientState::HandedOff])), 0);
    }

    #[test]
    fn an_unreadable_lease_is_not_reported_as_no_watch() {
        let unreadable = watch_state(Err(SendError::Store(StoreError::Corrupt(
            "bad lease row".to_owned(),
        ))));
        let line = watch_line(&team(), &unreadable, true);
        assert!(
            line.starts_with("could not tell whether a watch is running:"),
            "{line}"
        );
        assert!(line.contains("bad lease row"), "{line}");
        assert!(!line.contains("no watch is running"), "{line}");
        let none = watch_line(&team(), &watch_state(Ok(None)), true);
        assert!(none.starts_with("no watch is running"), "{none}");
        assert!(none.contains("`teamwire watch "), "{none}");
    }

    #[test]
    fn a_lead_that_cannot_read_watch_lines_is_told_to_deliver_in_the_background() {
        let line = watch_line(&team(), &watch_state(Ok(None)), false);
        assert!(
            line.contains("`nohup teamwire watch alpha --deliver >/dev/null 2>&1 &`"),
            "{line}"
        );
    }
}
