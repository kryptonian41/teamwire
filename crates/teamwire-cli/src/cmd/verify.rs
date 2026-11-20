//! `teamwire verify`: each member's check and the diff rules, and waivers.

use std::path::Path;
use std::process::ExitCode;

use teamwire_core::{Role, TeamName};
use teamwire_verify::{
    FileProblem, FlagId, Pin, VerifyError, VerifyOpts, VerifyRecord, VerifyReport,
};

use crate::cmd::shared::{
    Opening, PARTIAL, REFUSED, STORE, fail, refuse_member, say, warn, with_ctx_using,
};
use crate::wiring::Wiring;
use crate::{Global, USAGE, VerifyArgs};

pub(crate) fn run(args: &VerifyArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let VerifyArgs {
        team,
        role,
        waive,
        reason,
        deep,
        repair_files,
        pinned_in,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("verify", &format!("{e}"), USAGE),
    };
    let role = match role.as_deref().map(Role::parse).transpose() {
        Ok(role) => role,
        Err(e) => return fail("verify", &format!("{e}"), USAGE),
    };
    // A member who could waive its own flag would turn FLAGGED into PASS. A
    // plain run stays open, because the closer runs it from a member session.
    if waive.is_some()
        && let Some(code) = refuse_member("verify --waive")
    {
        return code;
    }
    let opening = Opening {
        command: "verify",
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "verify", opening, |ctx| {
        if let (Some(dir), Some(role)) = (pinned_in, &role) {
            return match teamwire_verify::pin(ctx, role, dir) {
                Ok(pin) => show_pin(&pin, &team),
                Err(e) => fail("verify", &e.to_string(), error_code(&e)),
            };
        }
        if let (Some(flag), Some(reason), Some(role)) = (waive, reason, &role) {
            return match teamwire_verify::waive(ctx, role, &FlagId::new(flag), reason) {
                Ok(()) => {
                    say(&format!("waived {flag} for {role} at its current HEAD\n"));
                    ExitCode::SUCCESS
                }
                Err(e) => fail("verify", &e.to_string(), error_code(&e)),
            };
        }
        let opts = VerifyOpts {
            deep: *deep,
            repair_files: *repair_files,
        };
        match teamwire_verify::run(ctx, role.as_ref(), opts) {
            Ok(report) => {
                say(&render(&report, ctx.home, role.is_some()));
                if report.is_clean(role.is_some()) {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(PARTIAL)
                }
            }
            Err(e) => fail("verify", &e.to_string(), error_code(&e)),
        }
    })
    .unwrap_or_else(|code| code)
}

/// The pin's lines, with the team named in its advice. Exit 0 only when the
/// clone sits on the commit verify recorded, or there is nothing to pin.
fn show_pin(pin: &Pin, team: &TeamName) -> ExitCode {
    let short = |sha: &Option<String>| sha.clone().unwrap_or_else(|| "unknown".to_owned());
    if let Some(why) = &pin.nothing_to_pin {
        say(&format!("{}: {why}\n", pin.role));
        return ExitCode::SUCCESS;
    }
    if pin.holds() {
        say(&format!(
            "pinned: {} at {} (verify {}); the clone at {} is on that commit. This proves the \
             commit only: each check still needs its own run there\n",
            pin.role,
            short(&pin.expected),
            pin.verdict.as_deref().unwrap_or("unknown"),
            pin.clone.display()
        ));
        return ExitCode::SUCCESS;
    }
    for problem in &pin.problems {
        warn(&format!(
            "NOT PINNED: {}",
            problem.replace("<team>", team.as_str())
        ));
    }
    warn("evidence from this clone is not evidence for the verified commit");
    ExitCode::from(REFUSED)
}

fn error_code(e: &VerifyError) -> u8 {
    match e {
        VerifyError::Store(_) => STORE,
        VerifyError::UnknownRole(_)
        | VerifyError::UnknownFlag(_)
        | VerifyError::Git(_)
        | VerifyError::Internal(_) => REFUSED,
    }
}

/// One verdict per member with its flags, then anything wrong with the
/// readable copies and the deep checks, then the summary line.
fn render(report: &VerifyReport, home: &Path, role_named: bool) -> String {
    let mut lines: Vec<String> = Vec::new();
    for record in &report.records {
        lines.extend(record_lines(record));
    }
    for path in &report.repaired {
        lines.push(format!("repaired {}", relative(path, home)));
    }
    for copy in &report.copies {
        let what = match copy.problem {
            FileProblem::Missing => "missing",
            FileProblem::Differs => "differs from the store",
        };
        let fix = if copy.repairable {
            "run with --repair-files to rewrite it"
        } else {
            "the store cannot rebuild it"
        };
        lines.push(format!(
            "copy {}: {what} ({fix})",
            relative(&copy.path, home)
        ));
    }
    if let Some(deep) = &report.deep {
        if deep.integrity == "ok" {
            lines.push("integrity_check: ok".to_owned());
        } else {
            lines.push(format!(
                "integrity_check found problems:\n{}",
                deep.integrity
            ));
        }
        for id in &deep.bad_hashes {
            lines.push(format!("message {id}: the body does not match its hash"));
        }
    }
    lines.push(String::new());
    lines.push(
        if report.is_clean(role_named) {
            "clean"
        } else {
            "NOT clean: read each flag; waive one with --role R --waive FLAG --reason TEXT"
        }
        .to_owned(),
    );
    lines.join("\n") + "\n"
}

fn record_lines(record: &VerifyRecord) -> Vec<String> {
    let exit = match (record.exit_code, record.seconds) {
        (Some(code), Some(seconds)) => format!(" exit {code} in {seconds}s,"),
        _ => String::new(),
    };
    let mut lines = vec![format!(
        "{:<12} {:<8}{exit} {} flag(s)",
        record.role.as_str(),
        record.verdict.as_str(),
        record.flags.len()
    )];
    for flag in &record.flags {
        let waived = if flag.waived { " (waived)" } else { "" };
        lines.push(format!(
            "             ! [{}]{waived} {}",
            flag.id, flag.detail
        ));
    }
    if let Some(note) = &record.note {
        lines.push(format!("             {note}"));
    }
    if let Some(head) = &record.head_sha {
        lines.push(format!("             head {head}"));
    }
    if let Some(log) = &record.log {
        lines.push(format!("             log {}", log.display()));
    }
    lines
}

fn relative(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .unwrap_or(path)
        .display()
        .to_string()
}
