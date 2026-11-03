//! `teamwire send`: send one message. Also holds what the message commands
//! (`resend`, `log`, `inbox`, `ack`, `members`, `lead`) share: the send
//! report's printing and exit codes.

use std::process::ExitCode;

use teamwire_core::{DisplayId, MessageType, Recipient, SendRequest, TeamName};
use teamwire_msg::{
    RecipientState, SendError, SendReport, parse_type_logged, prove_actor_logged, send_with,
    watch_running,
};
use teamwire_store::Party;

use crate::cmd::shared::{REFUSED, STORE, caller_env, fail, member_token, say, warn, with_ctx};
use crate::wiring::Wiring;
use crate::{Global, SendArgs, USAGE};

/// At least one recipient is `undelivered`.
const NOT_DELIVERED: u8 = 4;
/// At least one recipient is `unconfirmed`, none `undelivered`.
const UNCONFIRMED: u8 = 5;

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
    let outcome = with_ctx(global, wiring, &team, "send", |ctx| {
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
            // In a resume by key a reached row keeps its own state, so
            // `skipped` means another command may still be typing it.
            RecipientState::Skipped if report.duplicate_of_key => warn(&format!(
                "SKIPPED for {}: another command may still be delivering it; if it does not arrive, run teamwire resend {team} {} after 60 s",
                d.to, report.display_id
            )),
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
/// unconfirmed, or skipped in a resume by key because another command may
/// still be delivering it.
pub(crate) fn exit_of(report: &SendReport) -> u8 {
    let any = |s| report.deliveries.iter().any(|d| d.state == s);
    if any(RecipientState::Pending) {
        STORE
    } else if any(RecipientState::Undelivered) {
        NOT_DELIVERED
    } else if any(RecipientState::Unconfirmed)
        || (report.duplicate_of_key && any(RecipientState::Skipped))
    {
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

#[cfg(test)]
mod tests {
    use teamwire_store::{Store, StoreError};

    use super::*;

    #[test]
    fn every_command_on_with_ctx_records_a_failed_write_under_its_name() {
        let home = teamwire_fakes::TestHome::new().unwrap();
        drop(Store::open(home.path(), &team()).unwrap());
        let global = Global {
            home: Some(home.path().to_path_buf()),
        };
        let wiring = crate::wiring::wire(&crate::wiring::WiringEnv {
            herdr_env: None,
            tmux: None,
            user_home: None,
            claude_dir: None,
            claude_config_dir: None,
            codex_test_home: None,
            codex_home: None,
            pi_agent_dir: None,
            lead_harness: None,
            claude_session: None,
            codex_thread: None,
            pi_marker: None,
        })
        .unwrap();
        let out = with_ctx(&global, &wiring, &team(), "ack", |ctx| {
            ctx.store.write(|_| Err::<(), _>(StoreError::Busy))
        });
        assert!(matches!(out, Ok(Err(StoreError::Busy))));
        let store = Store::open_existing(home.path(), &team()).unwrap();
        let errors: Vec<serde_json::Value> = store
            .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == teamwire_core::EventType::StoreError)
            .map(|e| e.data)
            .collect();
        assert_eq!(errors, [serde_json::json!({ "op": "ack", "code": "busy" })]);
    }

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
