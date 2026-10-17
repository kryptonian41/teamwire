//! The command line as clap sees it: every subcommand's arguments.
//!
//! Never: hold logic. Each command's file reads its own struct.

use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "teamwire",
    version = teamwire_store::WRITER_VERSION,
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
    /// Resume the lead and the members of a team whose sessions are gone.
    Reopen(ReopenArgs),
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
    /// The team roster, of your own team or any other.
    Members(MembersArgs),
    /// Every team on this machine with its state and members. Reads only.
    Teams(TeamsArgs),
    /// Hook entry points, started by a harness.
    Hook(HookArgs),
    /// The stdio MCP server, started by a harness.
    Mcp(McpArgs),
    /// Read-only checks of the state, the disk and the tools teamwire needs.
    Doctor,
    /// Install the lead setup: the teamwire skill and the lead's MCP server.
    Install(InstallArgs),
    /// Remove the lead setup that `install` wrote.
    Uninstall(UninstallArgs),
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
pub(crate) struct ReopenArgs {
    pub(crate) team: String,
    /// Reopen only this member; the lead is left alone.
    #[arg(long, value_name = "R")]
    pub(crate) role: Option<String>,
    /// Print what would be reopened; open and write nothing.
    #[arg(long)]
    pub(crate) dry_run: bool,
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
    /// Rewrite missing or differing comms/ copies from the store; outbox/
    /// copies are reported, never rewritten.
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
    /// Recipients, separated by commas: a role or lead in this team, or
    /// role@team or lead@team in another team.
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
pub(crate) struct InstallArgs {
    /// Print the changes without making them.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Replace or remove a skill or MCP entry that is not teamwire's, and
    /// install from a binary inside a `target/` directory.
    #[arg(long)]
    pub(crate) force: bool,
}

#[derive(Args)]
pub(crate) struct UninstallArgs {
    /// Print the changes without making them.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Also remove a skill or MCP entry that is not teamwire's.
    #[arg(long)]
    pub(crate) force: bool,
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
pub(crate) struct TeamsArgs {
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct HookArgs {
    #[command(subcommand)]
    pub(crate) hook: HookCommand,
}

#[derive(Subcommand)]
#[expect(
    clippy::enum_variant_names,
    reason = "the variant names are the command names Claude Code's hook settings spell"
)]
pub(crate) enum HookCommand {
    /// A member's Stop hook: hold once for gaps or unread messages, else deliver the result.
    ClaudeStop(HookTarget),
    #[command(about = "A member's PostToolUse hook: remind it of unread message ids.")]
    ClaudePostTool(HookTarget),
    #[command(
        about = "The lead's Stop hook: hold once when a team has working members and no watch."
    )]
    ClaudeLeadStop,
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
    #[arg(long, requires = "role")]
    pub(crate) team: Option<String>,
    #[arg(long)]
    pub(crate) role: Option<String>,
    /// The member's token. Never printed. Without it the server reads
    /// `TEAMWIRE_TOKEN`, which keeps the token out of the process list.
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
        /// Export one pass and exit; without it, export every minute until stopped.
        #[arg(long)]
        once: bool,
    },
}
