//! The `teamwire` binary: argument parsing, wiring the concrete adapters,
//! and exit codes (`docs/api/cli.md`).
//!
//! Never: hold a rule or state logic (that lives in the service crates);
//! print or exit from any other crate.
//!
//! Invariants: exit codes are those of `docs/api/cli.md` (3 for a usage
//! error); a `hook` invocation always exits 0, even when its arguments do
//! not parse, so a hook never fails the harness's turn (REP-10).

use std::io::IsTerminal as _;
use std::process::ExitCode;

use clap::Parser;

mod cmd {
    pub(crate) mod ack;
    pub(crate) mod afk;
    pub(crate) mod close;
    pub(crate) mod decisions;
    pub(crate) mod doctor;
    pub(crate) mod doctor_speckit;
    pub(crate) mod doctor_speckit_files;
    pub(crate) mod doctor_teams;
    pub(crate) mod done;
    pub(crate) mod events;
    pub(crate) mod export;
    pub(crate) mod handoff;
    pub(crate) mod hook;
    pub(crate) mod host_pick;
    pub(crate) mod inbox;
    pub(crate) mod install;
    pub(crate) mod launch;
    pub(crate) mod lead;
    pub(crate) mod lead_setup_check;
    pub(crate) mod log;
    pub(crate) mod mcp;
    pub(crate) mod members;
    pub(crate) mod metrics;
    pub(crate) mod models;
    pub(crate) mod pr_watch;
    pub(crate) mod preset;
    pub(crate) mod received;
    pub(crate) mod reopen;
    pub(crate) mod resend;
    pub(crate) mod send;
    pub(crate) mod shared;
    pub(crate) mod status;
    pub(crate) mod teams;
    pub(crate) mod user_key;
    pub(crate) mod verify;
    pub(crate) mod version;
    pub(crate) mod watch;
}
mod args;
mod recorded;
mod wiring;

/// Exit code for a usage error.
const USAGE: u8 = 3;

pub(crate) use args::{
    AckArgs, AfkArgs, AfkCommand, AfkStartArgs, Cli, CloseArgs, Command, DecisionsArgs, DoneArgs,
    EventsArgs, ExportArgs, Global, HandoffArgs, HookArgs, HookCommand, HookTarget, InboxArgs,
    InstallArgs, LaunchArgs, LeadArgs, LogArgs, McpArgs, MembersArgs, MetricsArgs, MetricsCommand,
    ModelsArgs, PrWatchArgs, PresetArgs, ReceivedArgs, ReopenArgs, ResendArgs, SendArgs,
    StatusArgs, TeamsArgs, UninstallArgs, VerifyArgs, WatchArgs,
};

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(e) => return usage_error(&e, is_hook(&args)),
    };
    init_logging();
    let env = wiring::WiringEnv::from_process();
    let wiring = match wiring::wire_with_home(&env, cli.global.home.as_deref()) {
        Ok(wiring) => wiring,
        Err(e) => return internal_error(&e),
    };
    let global = &cli.global;
    match &cli.command {
        Command::Afk(a) => cmd::afk::run(a, global, &wiring),
        Command::Launch(a) => cmd::launch::run(a, global, &wiring),
        Command::Reopen(a) => cmd::reopen::run(a, global, &wiring),
        Command::Handoff(a) => cmd::handoff::run(a, global, &wiring),
        Command::Watch(a) => cmd::watch::run(a, global, &wiring),
        Command::Status(a) => cmd::status::run(a, global, &wiring),
        Command::Verify(a) => cmd::verify::run(a, global, &wiring),
        Command::Close(a) => cmd::close::run(a, global, &wiring),
        Command::Send(a) => cmd::send::run(a, global, &wiring),
        Command::Resend(a) => cmd::resend::run(a, global, &wiring),
        Command::Received(a) => cmd::received::run(a, global, &wiring),
        Command::Log(a) => cmd::log::run(a, global, &wiring),
        Command::Decisions(a) => cmd::decisions::run(a, global, &wiring),
        Command::Lead(a) => cmd::lead::run(a, global, &wiring),
        Command::PrWatch(a) => cmd::pr_watch::run(a, global, &wiring),
        Command::Preset(a) => cmd::preset::run(a, global, &wiring),
        Command::Models(a) => cmd::models::run(a, global, &wiring),
        Command::Done(a) => cmd::done::run(a, global, &wiring),
        Command::Inbox(a) => cmd::inbox::run(a, global, &wiring),
        Command::Ack(a) => cmd::ack::run(a, global, &wiring),
        Command::Members(a) => cmd::members::run(a, global, &wiring),
        Command::Teams(a) => cmd::teams::run(a, global, &wiring),
        Command::Hook(a) => cmd::hook::run(a, global, &wiring),
        Command::Mcp(a) => cmd::mcp::run(a, global, &wiring),
        Command::Doctor(a) => cmd::doctor::run(&a.base, global, &wiring),
        Command::Install(a) => cmd::install::run(a, global, &wiring),
        Command::Uninstall(a) => cmd::install::run_uninstall(a, global, &wiring),
        Command::Export(a) => cmd::export::run(a, global, &wiring),
        Command::Events(a) => cmd::events::run(a, global, &wiring),
        Command::Metrics(a) => cmd::metrics::run(a, global, &wiring),
        Command::UserKey => cmd::user_key::run(global, &wiring),
        Command::Version => cmd::version::run(global, &wiring),
    }
}

/// True when the first argument after the program that is not the global
/// `--home` option or its value is `hook`.
fn is_hook(args: &[std::ffi::OsString]) -> bool {
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        if arg == "--home" {
            rest.next();
        } else if !arg.to_string_lossy().starts_with("--home=") {
            return arg == "hook";
        }
    }
    false
}

/// Help and version go to stdout with exit 0; any other parse error is a
/// usage error, exit 3, except under `hook`, which always exits 0.
fn usage_error(e: &clap::Error, hook: bool) -> ExitCode {
    let printed = e.print();
    if hook || (printed.is_ok() && !e.use_stderr()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(USAGE)
    }
}

/// Diagnostics go to stderr, filtered by `TEAMWIRE_LOG` (default: warnings).
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("TEAMWIRE_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let installed = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        // Colour codes are noise to an agent reading stderr through a pipe.
        .with_ansi(std::io::stderr().is_terminal())
        .try_init();
    if installed.is_err() {
        tracing::debug!("a log subscriber was already installed");
    }
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports its own failure on stderr"
)]
fn internal_error(e: &dyn std::fmt::Display) -> ExitCode {
    eprintln!("teamwire: internal error: {e}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    fn parse(line: &str) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("teamwire").chain(line.split_whitespace()))
    }

    #[test]
    fn clap_definitions_are_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_documented_command_parses() {
        for line in [
            "afk start --team run --title Run --task t.md",
            "afk start --team run --title Run --task t.md --budget 90m",
            "launch",
            "launch --add --dry-run --spec s.json",
            "handoff tw --role api --dry-run",
            "handoff --guide",
            "watch tw --interval 5 --once",
            "status",
            "status tw --role api --json",
            "verify tw --role api --deep --repair-files",
            "verify tw --role api --waive WEAKENED:a.rs:3 --reason flaky",
            "close tw --role api --stop --unverified --dry-run --keep-state --archive-to /tmp/a --clear-stale-lock",
            "send tw --to api,web --type finding --text hi --commit abc --re m001 --key k1 --from lead",
            "send tw --to lead --type question --file q.md",
            "resend tw m014 --to api",
            "log tw --role api --json",
            "lead --adopt tw",
            "pr-watch -R o/r -i 30 123 124",
            "preset",
            "preset review",
            "models",
            "models --harness pi --for root_cause --json",
            "done --status done --summary finished",
            "inbox --all --json",
            "ack m001 m002",
            "members tw --json",
            "hook claude-stop --team tw --role api",
            "hook claude-post-tool",
            "mcp --team tw --role api --token t",
            "mcp --team tw --role api",
            "mcp --lead",
            "doctor",
            "doctor --base main",
            "install",
            "install --dry-run --force",
            "uninstall --dry-run --force",
            "export tw --out e.jsonl",
            "metrics tw --since 2h --json",
            "metrics export --once",
            "version",
            "--home /tmp/h status",
        ] {
            assert!(parse(line).is_ok(), "{line}: {:?}", parse(line).err());
        }
    }

    #[test]
    fn metrics_export_is_the_subcommand_not_a_team() {
        let cli = parse("metrics export --once").unwrap();
        let Command::Metrics(args) = cli.command else {
            unreachable!("parsed as another command");
        };
        assert!(args.team.is_none());
        assert!(matches!(
            args.action,
            Some(MetricsCommand::Export { once: true })
        ));
    }

    #[test]
    fn send_splits_recipients_on_commas() {
        let Command::Send(args) = parse("send tw --to api,web --type note --text x")
            .unwrap()
            .command
        else {
            unreachable!("parsed as another command");
        };
        assert_eq!(args.to, ["api", "web"]);
    }

    #[test]
    fn wrong_combinations_are_usage_errors() {
        for line in [
            "send tw --to api --type note",
            "send tw --to api --type note --text a --file b",
            "mcp",
            "mcp --lead --team tw",
            "mcp --team tw",
            "verify tw --waive F",
            "pr-watch -R o/r",
            "ack",
            "handoff tw",
            "handoff --role api",
            "handoff tw --role api --guide",
            "afk start --team run --title Run --task t.md --budget 0h",
            "afk start --team run --title Run --task t.md --budget 8",
            "afk start --team run --title Run --task t.md --budget 8d",
            "afk start --team run --title Run --task t.md --budget h",
            "afk start --team run --title Run --task t.md --budget 8é",
            "afk start --team run --title Run --task t.md --budget +8h",
            "afk start --team run --title Run --task t.md --budget -2h",
            "afk start --team run --title Run",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
    }

    #[test]
    fn only_hook_invocations_are_hooks() {
        let args = |line: &str| -> Vec<std::ffi::OsString> {
            line.split_whitespace().map(Into::into).collect()
        };
        assert!(is_hook(&args("teamwire hook claude-stop --bogus")));
        assert!(is_hook(&args(
            "teamwire --home /h hook claude-stop --bogus"
        )));
        assert!(is_hook(&args(
            "teamwire --home=/h hook claude-stop --bogus"
        )));
        assert!(!is_hook(&args("teamwire --home /h send hook")));
        assert!(!is_hook(&args("teamwire --home")));
        assert!(!is_hook(&args("teamwire send hook")));
        assert!(!is_hook(&args("teamwire")));
    }
}
