//! The `teamwire` binary: argument parsing, wiring the concrete adapters,
//! and exit codes (`docs/api/cli.md`).
//!
//! Never: hold a rule or state logic (that lives in the service crates);
//! print or exit from any other crate.
//!
//! Invariants: exit codes are those of `docs/api/cli.md` (3 for a usage
//! error); a `hook` invocation always exits 0, even when its arguments do
//! not parse, so a hook never fails the harness's turn (REP-10).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Args, Parser, Subcommand};

mod cmd {
    pub(crate) mod ack;
    pub(crate) mod close;
    pub(crate) mod doctor;
    pub(crate) mod done;
    pub(crate) mod export;
    pub(crate) mod hook;
    pub(crate) mod inbox;
    pub(crate) mod launch;
    pub(crate) mod lead;
    pub(crate) mod log;
    pub(crate) mod mcp;
    pub(crate) mod members;
    pub(crate) mod metrics;
    pub(crate) mod pr_watch;
    pub(crate) mod preset;
    pub(crate) mod resend;
    pub(crate) mod send;
    pub(crate) mod status;
    pub(crate) mod verify;
    pub(crate) mod version;
    pub(crate) mod watch;
}
mod wiring;

use wiring::Wiring;

/// Exit code for a usage error.
const USAGE: u8 = 3;

#[derive(Parser)]
#[command(
    name = "teamwire",
    version,
    about = "Runs teams of coding agents: launch, messages, completions, watch, verify, close."
)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) global: Global,
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// Flags every subcommand takes.
#[derive(Args)]
pub(crate) struct Global {
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        help = "State directory. Overrides $TEAMWIRE_HOME (default ~/.teamwire)."
    )]
    pub(crate) home: Option<PathBuf>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Validate a team spec, then start its members.
    Launch(LaunchArgs),
    /// Print one line when something needs the lead, until the team completes.
    Watch(WatchArgs),
    /// Show teams, or one team's members, messages and results. Reads only.
    Status(StatusArgs),
    /// Run each member's check and the diff rules; print a verdict per member.
    Verify(VerifyArgs),
    /// Close the team, or remove one member, in the fixed order.
    Close(CloseArgs),
    /// Send one message.
    Send(SendArgs),
    /// Retry one stored message to the recipients it did not reach.
    Resend(ResendArgs),
    /// Every message with its delivery state.
    Log(LogArgs),
    /// Record this session as the team's lead.
    Lead(LeadArgs),
    /// Watch GitHub pull-request checks until every PR is done.
    #[command(name = "pr-watch")]
    PrWatch(PrWatchArgs),
    /// List the team shapes, or print one.
    Preset(PresetArgs),
    /// Record that you are done, blocked or failed (members).
    Done(DoneArgs),
    /// Print unread messages and mark them read (members).
    Inbox(InboxArgs),
    /// Mark messages read without printing them (members).
    Ack(AckArgs),
    /// The team roster.
    Members(MembersArgs),
    /// Hook entry points, started by a harness.
    Hook(HookArgs),
    /// The stdio MCP server, started by a harness.
    Mcp(McpArgs),
    /// Read-only checks of the state, the disk and the tools teamwire needs.
    Doctor,
    /// Every record of a team as JSON lines.
    Export(ExportArgs),
    /// Local metrics from the event rows, or the OpenTelemetry export.
    Metrics(MetricsArgs),
    /// The binary's version and the schema version it writes.
    Version,
}

#[derive(Args)]
pub(crate) struct LaunchArgs {
    /// Add members to a running team.
    #[arg(long)]
    pub(crate) add: bool,
    /// Validate and print the plan; touch nothing.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Read the spec from FILE instead of stdin.
    #[arg(long, value_name = "FILE")]
    pub(crate) spec: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct WatchArgs {
    pub(crate) team: String,
    /// Seconds between polls.
    #[arg(long, value_name = "SECONDS", default_value_t = 20)]
    pub(crate) interval: u64,
    /// Run one poll and exit.
    #[arg(long)]
    pub(crate) once: bool,
}

#[derive(Args)]
pub(crate) struct StatusArgs {
    pub(crate) team: Option<String>,
    /// Also print the command that resumes this member's session.
    #[arg(long, value_name = "R")]
    pub(crate) role: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct VerifyArgs {
    pub(crate) team: String,
    #[arg(long, value_name = "R")]
    pub(crate) role: Option<String>,
    /// Waive one flag of one member at its current HEAD.
    #[arg(long, value_name = "FLAG", requires_all = ["reason", "role"])]
    pub(crate) waive: Option<String>,
    /// Why the flag is waived.
    #[arg(long, value_name = "TEXT", requires = "waive")]
    pub(crate) reason: Option<String>,
    /// Also run the database integrity check and check every message hash.
    #[arg(long)]
    pub(crate) deep: bool,
    /// Rewrite missing or differing comms/ and outbox/ copies from the store.
    #[arg(long)]
    pub(crate) repair_files: bool,
}

#[derive(Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each field is one command-line flag of `teamwire close`"
)]
pub(crate) struct CloseArgs {
    pub(crate) team: String,
    /// Remove one member instead of closing the team.
    #[arg(long, value_name = "R")]
    pub(crate) role: Option<String>,
    /// Allow removing a busy member.
    #[arg(long)]
    pub(crate) stop: bool,
    /// Remove an implementer without a clean verify, and print why.
    #[arg(long)]
    pub(crate) unverified: bool,
    /// Print the plan; change nothing.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Leave the team's state directory in place, marked closed.
    #[arg(long)]
    pub(crate) keep_state: bool,
    /// Put the archive in DIR.
    #[arg(long, value_name = "DIR")]
    pub(crate) archive_to: Option<PathBuf>,
    /// Clear a close lock whose owner pid is dead, and nothing else.
    #[arg(long)]
    pub(crate) clear_stale_lock: bool,
}

#[derive(Args)]
#[command(group(ArgGroup::new("body").required(true).args(["text", "file"])))]
pub(crate) struct SendArgs {
    #[arg(
        env = "TEAMWIRE_TEAM",
        help = "The team. A member's defaults to $TEAMWIRE_TEAM."
    )]
    pub(crate) team: String,
    /// Recipients: roles separated by commas, or lead.
    #[arg(long, value_name = "ROLES", value_delimiter = ',', required = true)]
    pub(crate) to: Vec<String>,
    /// notice, finding, question, answer, reprompt or note.
    #[arg(long = "type", value_name = "TYPE")]
    pub(crate) kind: String,
    #[arg(long, value_name = "TEXT")]
    pub(crate) text: Option<String>,
    /// Read the message text from PATH.
    #[arg(long, value_name = "PATH")]
    pub(crate) file: Option<PathBuf>,
    /// Required for a notice: the commit it announces.
    #[arg(long, value_name = "SHA")]
    pub(crate) commit: Option<String>,
    /// Required for an answer: the question's id.
    #[arg(long, value_name = "ID")]
    pub(crate) re: Option<String>,
    /// Idempotency key: the same key never makes a second message.
    #[arg(long, value_name = "KEY")]
    pub(crate) key: Option<String>,
    #[arg(
        long,
        value_name = "ROLE|lead",
        env = "TEAMWIRE_ROLE",
        help = "The sender: your role, or lead. A member's defaults to $TEAMWIRE_ROLE."
    )]
    pub(crate) from: Option<String>,
}

#[derive(Args)]
pub(crate) struct ResendArgs {
    pub(crate) team: String,
    /// The message's display id, such as m014.
    pub(crate) id: String,
    /// Only these recipients, separated by commas.
    #[arg(long, value_name = "ROLES", value_delimiter = ',')]
    pub(crate) to: Option<Vec<String>>,
}

#[derive(Args)]
pub(crate) struct LogArgs {
    pub(crate) team: String,
    /// Only messages to or from this role.
    #[arg(long)]
    pub(crate) role: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct LeadArgs {
    /// Record the calling session as this team's lead.
    #[arg(long, value_name = "TEAM")]
    pub(crate) adopt: String,
}

#[derive(Args)]
pub(crate) struct PrWatchArgs {
    #[arg(short = 'R', long = "repo", value_name = "OWNER/REPO")]
    pub(crate) repo: String,
    /// Seconds between polls.
    #[arg(short = 'i', long = "interval", value_name = "SECONDS")]
    pub(crate) interval: Option<u64>,
    #[arg(value_name = "PR", required = true)]
    pub(crate) prs: Vec<u64>,
}

#[derive(Args)]
pub(crate) struct PresetArgs {
    pub(crate) name: Option<String>,
}

#[derive(Args)]
pub(crate) struct DoneArgs {
    /// done, blocked or failed.
    #[arg(long)]
    pub(crate) status: String,
    #[arg(long, value_name = "TEXT")]
    pub(crate) summary: String,
}

#[derive(Args)]
pub(crate) struct InboxArgs {
    /// Also show read messages, and mark nothing.
    #[arg(long)]
    pub(crate) all: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct AckArgs {
    #[arg(value_name = "ID", required = true)]
    pub(crate) ids: Vec<String>,
}

#[derive(Args)]
pub(crate) struct MembersArgs {
    pub(crate) team: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct HookArgs {
    #[command(subcommand)]
    pub(crate) hook: HookCommand,
}

#[derive(Subcommand)]
pub(crate) enum HookCommand {
    /// A member's Stop hook: hold once for gaps or unread messages, else deliver the result.
    ClaudeStop(HookTarget),
    #[command(about = "A member's PostToolUse hook: remind it of unread message ids.")]
    ClaudePostTool(HookTarget),
}

/// Who the hook runs for. The per-run settings put both in the command.
#[derive(Args)]
pub(crate) struct HookTarget {
    #[arg(long)]
    pub(crate) team: Option<String>,
    #[arg(long)]
    pub(crate) role: Option<String>,
}

#[derive(Args)]
#[command(group(ArgGroup::new("caller").required(true).args(["team", "lead"])))]
pub(crate) struct McpArgs {
    #[arg(long, requires_all = ["role", "token"])]
    pub(crate) team: Option<String>,
    #[arg(long)]
    pub(crate) role: Option<String>,
    /// The member's token. Never printed.
    #[arg(long)]
    pub(crate) token: Option<String>,
    /// Serve the lead, proven by its harness session.
    #[arg(long, conflicts_with_all = ["team", "role", "token"])]
    pub(crate) lead: bool,
}

#[derive(Args)]
pub(crate) struct ExportArgs {
    pub(crate) team: String,
    /// Write to FILE instead of stdout.
    #[arg(long, value_name = "FILE")]
    pub(crate) out: Option<PathBuf>,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct MetricsArgs {
    pub(crate) team: Option<String>,
    /// Only events newer than this, such as 30m or 2h.
    #[arg(long, value_name = "DURATION")]
    pub(crate) since: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
    #[command(subcommand)]
    pub(crate) action: Option<MetricsCommand>,
}

#[derive(Subcommand)]
pub(crate) enum MetricsCommand {
    /// Send events after the export mark to the OTLP endpoint, then move the mark.
    Export {
        /// Export once and exit.
        #[arg(long)]
        once: bool,
    },
}

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(e) => return usage_error(&e, is_hook(&args)),
    };
    init_logging();
    let wiring = match wiring::wire(&wiring::WiringEnv::from_process()) {
        Ok(wiring) => wiring,
        Err(e) => return internal_error(&e),
    };
    let global = &cli.global;
    match &cli.command {
        Command::Launch(a) => cmd::launch::run(a, global, &wiring),
        Command::Watch(a) => cmd::watch::run(a, global, &wiring),
        Command::Status(a) => cmd::status::run(a, global, &wiring),
        Command::Verify(a) => cmd::verify::run(a, global, &wiring),
        Command::Close(a) => cmd::close::run(a, global, &wiring),
        Command::Send(a) => cmd::send::run(a, global, &wiring),
        Command::Resend(a) => cmd::resend::run(a, global, &wiring),
        Command::Log(a) => cmd::log::run(a, global, &wiring),
        Command::Lead(a) => cmd::lead::run(a, global, &wiring),
        Command::PrWatch(a) => cmd::pr_watch::run(a, global, &wiring),
        Command::Preset(a) => cmd::preset::run(a, global, &wiring),
        Command::Done(a) => cmd::done::run(a, global, &wiring),
        Command::Inbox(a) => cmd::inbox::run(a, global, &wiring),
        Command::Ack(a) => cmd::ack::run(a, global, &wiring),
        Command::Members(a) => cmd::members::run(a, global, &wiring),
        Command::Hook(a) => cmd::hook::run(a, global, &wiring),
        Command::Mcp(a) => cmd::mcp::run(a, global, &wiring),
        Command::Doctor => cmd::doctor::run(global, &wiring),
        Command::Export(a) => cmd::export::run(a, global, &wiring),
        Command::Metrics(a) => cmd::metrics::run(a, global, &wiring),
        Command::Version => cmd::version::run(global, &wiring),
    }
}

/// True when the first argument after the program is `hook`.
fn is_hook(args: &[std::ffi::OsString]) -> bool {
    args.get(1).is_some_and(|a| a == "hook")
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

/// What every command that is not built yet does: say so on stderr and exit
/// 3. A hook passes `exit_zero`, because a hook never fails a turn.
#[expect(clippy::print_stderr, reason = "the binary reports on stderr")]
pub(crate) fn not_built(
    command: &str,
    global: &Global,
    wiring: &Wiring,
    exit_zero: bool,
) -> ExitCode {
    let Global { home } = global;
    tracing::debug!(
        command,
        home = ?home,
        lead_harness = %wiring.lead_harness,
        lead_host = %wiring.lead_host,
        harnesses = ?wiring.registry.harness_ids(),
        hosts = ?wiring.registry.host_ids(),
        sleep_hold = wiring.platform.sleep_hold().is_some(),
        clock = ?wiring.clock,
        "command not built yet"
    );
    eprintln!("teamwire {command}: not built yet");
    if exit_zero {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(USAGE)
    }
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
            "launch",
            "launch --add --dry-run --spec s.json",
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
            "done --status done --summary finished",
            "inbox --all --json",
            "ack m001 m002",
            "members tw --json",
            "hook claude-stop --team tw --role api",
            "hook claude-post-tool",
            "mcp --team tw --role api --token t",
            "mcp --lead",
            "doctor",
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
        assert!(!is_hook(&args("teamwire send hook")));
        assert!(!is_hook(&args("teamwire")));
    }
}
