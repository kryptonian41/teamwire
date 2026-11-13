//! `teamwire amend`: change one started member's check or owns (ADR 0046).
//! The new scope and the `CHECK AMENDED:` note that records it are stored in
//! one transaction, then the note is delivered and the member's seat rules
//! file is rewritten, so a handoff or reopen reads the new scope.
//!
//! A lead command and a shell command only: a member that could amend its
//! own check could make its own verify pass.

use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_core::{
    AMEND_MARKER, Actor, Member, MemberState, MessageType, Recipient, Role, SendRequest, TeamName,
    TeamState,
};
use teamwire_msg::{RecipientState, SendReport, send_with_write};
use teamwire_store::Ctx;

use crate::cmd::send::{error_code, exit_of, pending_line, recipient_line};
use crate::cmd::shared::{
    Opening, PARTIAL, REFUSED, STORE, fail, refuse_member, say, warn, with_ctx_using,
};
use crate::wiring::Wiring;
use crate::{AmendArgs, Global, USAGE};

pub(crate) fn run(args: &AmendArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let AmendArgs {
        team,
        role,
        check,
        owns,
        reason,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("amend", &e.to_string(), USAGE),
    };
    let role = match Role::parse(role) {
        Ok(role) => role,
        Err(e) => return fail("amend", &e.to_string(), USAGE),
    };
    if let Some(code) = refuse_member("amend") {
        return code;
    }
    if reason.trim().is_empty() {
        return fail("amend", "--reason is empty", USAGE);
    }
    if check.as_deref().is_some_and(|c| c.trim().is_empty()) {
        return fail("amend", "--check is empty", USAGE);
    }
    // One whole pattern per flag, so an empty one has no meaning. Clearing
    // the owns list is not something amend does.
    if owns.iter().any(|o| o.trim().is_empty()) {
        return fail("amend", "--owns has an empty pattern", USAGE);
    }
    let change = Change {
        check: check.as_deref(),
        owns: (!owns.is_empty()).then_some(owns.as_slice()),
        reason,
    };
    let opening = Opening {
        command: "amend",
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "amend", opening, |ctx| {
        amend(ctx, &team, &role, &change)
    })
    .unwrap_or_else(|code| code)
}

/// What the lead asked to change. A field left `None` keeps its value.
struct Change<'a> {
    check: Option<&'a str>,
    owns: Option<&'a [String]>,
    reason: &'a str,
}

fn amend(ctx: &mut Ctx<'_>, team: &TeamName, role: &Role, change: &Change<'_>) -> ExitCode {
    let read = ctx.store.read(|tx| Ok((tx.team()?, tx.member(role)?)));
    let (team_row, member) = match read {
        Ok(rows) => rows,
        Err(e) => return fail("amend", &format!("the team cannot be read: {e}"), STORE),
    };
    if team_row.is_none_or(|t| t.state != TeamState::Open) {
        return fail("amend", &format!("team {team} is not open"), REFUSED);
    }
    let Some(member) = member else {
        return fail(
            "amend",
            &format!("no member {role} in team {team}"),
            REFUSED,
        );
    };
    if member.state != MemberState::Started {
        // Launch is still writing the scope of a creating or failed member,
        // and a removed one has no session to tell.
        return fail(
            "amend",
            &format!("{role} is {}, not started", member.state.as_str()),
            REFUSED,
        );
    }
    let mut amended = member.clone();
    if let Some(check) = change.check {
        amended.check = Some(check.to_owned());
    }
    if let Some(owns) = change.owns {
        amended.owns = owns.to_vec();
    }
    let lines = field_lines(&member, &amended, change);
    let request = SendRequest {
        team: team.clone(),
        actor: Actor::Lead,
        to: vec![Recipient::Member(role.clone())],
        kind: MessageType::Note,
        body: note_body(change.reason, &lines),
        commit: None,
        re: None,
        client_key: None,
    };
    let sent = send_with_write(
        ctx,
        request,
        &|tx| tx.set_member_scope(role, amended.check.as_deref(), &amended.owns),
        &mut |_| {},
    );
    let report = match sent {
        Ok(report) => report,
        Err(e) => return fail("amend", &e.to_string(), error_code(&e)),
    };
    let rewritten = std::env::current_exe()
        .map_err(|e| format!("the teamwire binary cannot be found: {e}"))
        .and_then(|bin| {
            teamwire_launch::rewrite_contract(ctx.platform, ctx.home, team, &amended, &bin)
        });
    say(&output(team, role, change, &lines, &report));
    for line in not_recorded(team, &report) {
        warn(&line);
    }
    match rewritten {
        Ok(_) => ExitCode::from(exit_of(&report)),
        Err(e) => {
            // A wrong seat rules file misleads every later handoff, so this
            // exit wins over an undelivered note's.
            warn(&format!(
                "error: amend: the seat rules file {} could not be rewritten ({e}); the check \
                 and owns are saved; run the same amend again",
                contract_path(ctx, team, role).display()
            ));
            ExitCode::from(PARTIAL)
        }
    }
}

/// The path `rewrite_contract` writes, named in the error when it fails.
fn contract_path(ctx: &Ctx<'_>, team: &TeamName, role: &Role) -> PathBuf {
    ctx.home
        .join("teams")
        .join(team.as_str())
        .join("contract")
        .join(format!("{}.md", role.as_str()))
}

/// One line per field the lead named, old value then new.
fn field_lines(old: &Member, new: &Member, change: &Change<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    if change.check.is_some() {
        let shown = |c: &Option<String>| c.clone().unwrap_or_else(|| "(none)".to_owned());
        lines.push(format!(
            "check: {} -> {}",
            shown(&old.check),
            shown(&new.check)
        ));
    }
    if change.owns.is_some() {
        lines.push(format!(
            "owns: [{}] -> [{}]",
            old.owns.join(", "),
            new.owns.join(", ")
        ));
    }
    lines
}

/// The note: the marker and the reason first, so `teamwire log`'s excerpt
/// shows both, then the changes, then what the member does next.
fn note_body(reason: &str, lines: &[String]) -> String {
    format!(
        "{AMEND_MARKER} {reason}\nThe lead changed your scope.\n{}\nVerify now runs the new \
         check. Run it yourself, then record your result again with teamwire done.\n",
        lines.join("\n")
    )
}

fn output(
    team: &TeamName,
    role: &Role,
    change: &Change<'_>,
    lines: &[String],
    report: &SendReport,
) -> String {
    let fields = match (change.check.is_some(), change.owns.is_some()) {
        (true, true) => "check and owns",
        (true, false) => "check",
        (false, _) => "owns",
    };
    let mut out = vec![format!(
        "amended {role} in team {team}: {fields} (note {})\n",
        report.display_id
    )];
    out.extend(lines.iter().map(|line| format!("  {line}\n")));
    out.extend(
        report
            .deliveries
            .iter()
            .filter(|d| !matches!(d.state, RecipientState::HandedOff | RecipientState::Acked))
            .map(|d| {
                recipient_line(&report.display_id, d)
                    .trim_start()
                    .to_owned()
            }),
    );
    out.push(format!(
        "run `teamwire verify {team} --role {role}` again: close --role counts only a verify \
         recorded after this amend\n"
    ));
    out.concat()
}

/// `send`'s stderr line for each recipient still `pending` after the
/// commit: its delivery record could not be written, so the exit is 6 with
/// the scope and the note saved, and `teamwire resend` delivers the note.
fn not_recorded(team: &TeamName, report: &SendReport) -> Vec<String> {
    report
        .deliveries
        .iter()
        .filter(|d| d.state == RecipientState::Pending)
        .map(|d| pending_line(team, &report.display_id, &d.to))
        .collect()
}

#[cfg(test)]
mod tests {
    use teamwire_core::{DeliveryErrorCode, DisplayId};
    use teamwire_msg::RecipientReport;

    use super::*;

    fn report(state: RecipientState, reason: Option<DeliveryErrorCode>) -> SendReport {
        SendReport {
            id: teamwire_core::Ulid::parse("01M466HBGM3C8KG2XPMZWRQCH4").unwrap(),
            display_id: DisplayId::parse("m007").unwrap(),
            seq: 7,
            deliveries: vec![RecipientReport {
                to: Recipient::parse("api").unwrap(),
                state,
                reason,
                attempts: None,
            }],
            duplicate_of_key: false,
        }
    }

    /// A note still `pending` after the commit exits 6 and says to resend
    /// it, as `send` does; the scope and the note are already saved.
    #[test]
    fn a_pending_note_exits_6_and_names_resend() {
        let team = TeamName::parse("alpha").unwrap();
        let pending = report(RecipientState::Pending, None);
        assert_eq!(exit_of(&pending), STORE);
        assert_eq!(
            not_recorded(&team, &pending),
            [
                "NOT RECORDED for api: the delivery record could not be written (store error); \
              run teamwire resend alpha m007"
            ]
        );
        for other in [
            report(RecipientState::HandedOff, None),
            report(
                RecipientState::Undelivered,
                Some(DeliveryErrorCode::NoLiveProcess),
            ),
            report(
                RecipientState::Undelivered,
                Some(DeliveryErrorCode::HeldForHandoff),
            ),
            report(RecipientState::Unconfirmed, None),
        ] {
            assert_eq!(
                not_recorded(&team, &other),
                Vec::<String>::new(),
                "{other:?}"
            );
        }
    }

    /// A note held for the member's fresh session exits 0, and `send`'s
    /// held line prints after the field lines, unindented like the other
    /// delivery lines of amend.
    #[test]
    fn a_held_note_prints_the_held_line_after_the_fields() {
        let team = TeamName::parse("alpha").unwrap();
        let role = Role::parse("api").unwrap();
        let held = report(
            RecipientState::Undelivered,
            Some(DeliveryErrorCode::HeldForHandoff),
        );
        let change = Change {
            check: Some("cargo test -p teamwire-store --test member"),
            owns: None,
            reason: "the old check ran every test file",
        };
        let lines = [
            "check: cargo test -p teamwire-store -> cargo test -p teamwire-store --test member"
                .to_owned(),
        ];
        assert_eq!(exit_of(&held), 0);
        assert_eq!(
            output(&team, &role, &change, &lines, &held),
            "amended api in team alpha: check (note m007)\n  \
             check: cargo test -p teamwire-store -> cargo test -p teamwire-store --test member\n\
             api: held for handoff (m007)\n\
             run `teamwire verify alpha --role api` again: close --role counts only a verify \
             recorded after this amend\n"
        );
    }
}
