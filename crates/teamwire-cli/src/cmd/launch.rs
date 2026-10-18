//! `teamwire launch`: validate a team spec, then start its members.

use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_core::{Clock, Config};
use teamwire_launch::{LaunchOutcome, ValidPlan, ValidateEnv, launch, parse_spec, validate};
use teamwire_store::{Ctx, Store};

use crate::cmd::send::{home_or_refuse, refuse_member};
use crate::wiring::Wiring;
use crate::{Global, LaunchArgs};

/// Exit codes of `docs/api/cli.md`.
const REFUSED: u8 = 1;
const PARTIAL: u8 = 2;
const STORE_ERROR: u8 = 6;

fn read_spec(args: &LaunchArgs) -> Result<String, String> {
    if let Some(file) = &args.spec {
        return std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read the spec {}: {e}", file.display()));
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read the spec from standard input: {e}"))?;
    Ok(text)
}

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the binary reports on stdout and stderr"
)]
pub(crate) fn run(args: &LaunchArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    if args.add
        && let Some(code) = refuse_member("launch")
    {
        return code;
    }
    let home = match home_or_refuse("launch", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let text = match read_spec(args) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("teamwire launch: {e}");
            return ExitCode::from(REFUSED);
        }
    };
    // Members get the OTLP export settings in their environment (OBS-4).
    let mut config = Config::default();
    if let Some(otlp) = teamwire_otel::load_otlp_config(&home, &|key| std::env::var(key).ok()) {
        config.otel.endpoint = Some(otlp.endpoint);
        config.otel.headers = otlp.headers;
    }
    let env = ValidateEnv {
        home: &home,
        registry: &wiring.registry,
        platform: &*wiring.platform,
        clock: &wiring.clock,
        config: &config,
        lead_harness: wiring.lead_harness.clone(),
        lead_host: wiring.lead_host.clone(),
        user_home: std::env::var_os("HOME").map(PathBuf::from),
        teamwire_bin: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("teamwire")),
    };
    let checked = parse_spec(&text).and_then(|spec| validate(&spec, args.add, &env));
    let plan = match checked {
        Ok(plan) => plan,
        Err(errors) => {
            for e in &errors {
                let who = e
                    .role
                    .as_deref()
                    .map_or(String::new(), |r| format!(" [{r}]"));
                eprintln!("refused{who}: {}: {}", e.code, e.message);
            }
            eprintln!("nothing was created");
            return ExitCode::from(REFUSED);
        }
    };
    // Only the plan's own members: `--add` does not recheck the ones already running.
    let found = crate::cmd::install::found(wiring);
    let missing: Vec<_> = plan
        .members
        .iter()
        .filter(|m| !found.contains(&m.harness.as_str()))
        .collect();
    if !missing.is_empty() {
        for m in missing {
            eprintln!(
                "refused: member {}: harness {} is not on PATH",
                m.spec.role, m.harness
            );
        }
        eprintln!("nothing was created");
        return ExitCode::from(REFUSED);
    }
    for warning in &plan.warnings {
        eprintln!("warning: {warning}");
    }
    if args.dry_run {
        print_plan(&plan);
        return ExitCode::SUCCESS;
    }
    let team = plan.spec.team.clone();
    let printed = plan.warnings.len();
    let mut store = match Store::open(&home, &team) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("teamwire launch: the store could not be opened: {e}");
            return ExitCode::from(STORE_ERROR);
        }
    };
    let report = {
        let mut ctx = Ctx {
            home: &home,
            store: &mut store,
            registry: &wiring.registry,
            platform: &*wiring.platform,
            clock: &wiring.clock as &dyn Clock,
            config: &config,
        };
        launch(&mut ctx, plan)
    };
    for m in &report.members {
        match &m.outcome {
            LaunchOutcome::Started => println!("{}: started", m.role),
            LaunchOutcome::Refused(why) => println!("{}: refused: {why}", m.role),
            LaunchOutcome::Failed(why) => println!("{}: failed: {why}", m.role),
        }
    }
    let started = report
        .members
        .iter()
        .filter(|m| matches!(m.outcome, LaunchOutcome::Started))
        .count();
    println!(
        "team {team}: {started}/{} members started",
        report.members.len()
    );
    // The plan's own warnings were printed before launching.
    for w in report.warnings.iter().skip(printed) {
        eprintln!("warning: {w}");
    }
    println!("watch: {}", report.watch_command);
    if started == report.members.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(PARTIAL)
    }
}

/// What `--dry-run` shows: the team, and each member as it would launch.
#[expect(clippy::print_stdout, reason = "the binary reports on stdout")]
fn print_plan(plan: &ValidPlan) {
    let verb = if plan.add { "add to" } else { "launch" };
    println!("dry run: would {verb} team {}", plan.spec.team);
    for m in &plan.members {
        let place = match &m.worktree {
            Some(w) => format!("worktree {} on branch {}", w.path.display(), w.branch),
            None => format!("in {}", m.cwd.display()),
        };
        let again = if m.relaunch { ", launched again" } else { "" };
        println!(
            "  {} ({}, {}) {} via {}, identity by {}{again}",
            m.spec.role, m.kind, m.spec.mode, place, m.host, m.channel
        );
    }
    println!("nothing was created");
}
