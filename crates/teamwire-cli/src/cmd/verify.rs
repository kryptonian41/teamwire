//! `teamwire verify`: each member's check and the diff rules, and waivers.

use std::path::Path;
use std::process::ExitCode;

use teamwire_core::{Config, Role, TeamName};
use teamwire_store::{Ctx, Store, StoreError};
use teamwire_verify::{FileProblem, FlagId, VerifyError, VerifyOpts, VerifyRecord, VerifyReport};

use crate::cmd::send::{
    PARTIAL, REFUSED, STORE, fail, home_or_refuse, no_database_text, refuse_member, say,
};
use crate::recorded::Recorded;
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
    let home = match home_or_refuse("verify", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let mut store = match Store::open_existing(&home, &team) {
        Ok(store) => Recorded::new(store, &wiring.clock, "verify"),
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(&team, &path);
            return fail("verify", &message, REFUSED);
        }
        Err(e) => return fail("verify", &e.to_string(), STORE),
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
    if let (Some(flag), Some(reason), Some(role)) = (waive, reason, &role) {
        return match teamwire_verify::waive(&mut ctx, role, &FlagId::new(flag), reason) {
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
    match teamwire_verify::run(&mut ctx, role.as_ref(), opts) {
        Ok(report) => {
            say(&render(&report, &home, role.is_some()));
            if report.is_clean(role.is_some()) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(PARTIAL)
            }
        }
        Err(e) => fail("verify", &e.to_string(), error_code(&e)),
    }
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
