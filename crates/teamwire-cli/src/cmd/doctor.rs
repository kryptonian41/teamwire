//! `teamwire doctor`: read-only checks of state, disk and tools.
//!
//! Every check only reads: databases open read-only, the herdr hook file is
//! only read, and `git` is asked a question, never told to change anything.
//! The checks take the platform as a value so a test can give a network file
//! system, a full disk or a dead process without a real one.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    Clock, Command, Config, Pid, Platform, Registry, ReportState, TeamName, TeamState,
};
use teamwire_store::{Ctx, SCHEMA_VERSION, Store};

use crate::Global;
use crate::cmd::install::wanted_entry;
use crate::cmd::lead_setup_check::{self, Outcome};
use crate::cmd::send::{PARTIAL, home_or_refuse, say};
use crate::wiring::{ClaudeConfig, CodexSetup, McpEntry, PiSetup, Wiring};

/// Launch refuses to start below this much free space (LCH-2).
const MIN_FREE_BYTES: u64 = 200 * 1024 * 1024;

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Each probe of a harness or host binary waits this long, then warns.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The flags teamwire passes when it starts a member
/// (`teamwire-harness-claude/src/launch.rs`). A `claude` whose help does not
/// list one may not support it; the launch itself is the real test.
const CLAUDE_FLAGS: [&str; 8] = [
    "--session-id",
    "--resume",
    "--permission-mode",
    "--append-system-prompt-file",
    "--settings",
    "--mcp-config",
    "--model",
    "--effort",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Check {
    pub(crate) level: Level,
    pub(crate) name: String,
    pub(crate) detail: String,
}

impl Check {
    fn new(level: Level, name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level,
            name: name.into(),
            detail: detail.into(),
        }
    }

    fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Level::Pass, name, detail)
    }

    fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Level::Warn, name, detail)
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Level::Fail, name, detail)
    }

    fn line(&self) -> String {
        let tag = match self.level {
            Level::Pass => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        };
        format!("{tag} {}: {}", self.name, self.detail)
    }
}

/// The host teamwire starts members in: herdr inside a herdr pane
/// (`HERDR_ENV=1`), else tmux inside a tmux session (`TMUX` set), else none.
/// Only the tools of the host in use are required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Host {
    Herdr,
    Tmux,
    None,
}

impl Host {
    fn id(self) -> &'static str {
        match self {
            Self::Herdr => "herdr",
            Self::Tmux => "tmux",
            Self::None => "none",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "herdr" => Self::Herdr,
            "tmux" => Self::Tmux,
            _ => Self::None,
        }
    }

    pub(crate) fn detect(herdr_env: Option<&str>, tmux: Option<&str>) -> Self {
        if herdr_env == Some("1") {
            Self::Herdr
        } else if tmux.is_some_and(|t| !t.is_empty()) {
            Self::Tmux
        } else {
            Self::None
        }
    }
}

/// The most parents `nearest_host` looks at. A loop in the chain stops here.
const MAX_PARENT_STEPS: usize = 64;

/// The first of `start` and its parents, nearest first, whose program is
/// `tmux` or `herdr`. `parent_of` gives a pid's parent and its own program
/// name; a failed lookup ends the walk.
fn nearest_host(
    start: Pid,
    parent_of: &dyn Fn(Pid) -> Option<(Pid, String)>,
) -> Option<&'static str> {
    let mut pid = start;
    for _ in 0..MAX_PARENT_STEPS {
        let (parent, name) = parent_of(pid)?;
        // Linux names the tmux server "tmux: server".
        match name
            .split_whitespace()
            .next()
            .map(|n| n.trim_end_matches(':'))
        {
            Some("tmux") => return Some("tmux"),
            Some("herdr") => return Some("herdr"),
            _ => {}
        }
        pid = parent;
    }
    None
}

/// The lead's host and the reason for the pick, for `doctor`.
pub(crate) struct HostPick {
    pub(crate) id: &'static str,
    pub(crate) why: String,
}

/// `herdr` inside a herdr pane (`HERDR_ENV=1`), else `tmux` inside a tmux
/// client (`TMUX` set), otherwise `none`. A tmux server started from a herdr
/// pane copies `HERDR_ENV` into its windows, and herdr started inside tmux
/// carries `TMUX`, so when both are set the nearest `tmux` or `herdr`
/// among this process's parents decides; with none found, herdr wins.
pub(crate) fn pick_host(
    herdr_env: Option<&str>,
    tmux: Option<&str>,
    parent_of: &dyn Fn(Pid) -> Option<(Pid, String)>,
) -> HostPick {
    // The rule when the parents cannot tell: herdr first.
    let first = Host::detect(herdr_env, tmux).id();
    let both = herdr_env == Some("1") && tmux.is_some_and(|t| !t.is_empty());
    let (id, why) = match (both, first) {
        (false, "herdr") => (first, "HERDR_ENV is 1".to_owned()),
        (false, "tmux") => (first, "TMUX is set".to_owned()),
        (false, _) => (first, "neither HERDR_ENV nor TMUX is set".to_owned()),
        (true, _) => match nearest_host(Pid::new(std::process::id()), parent_of) {
            Some(id) => (
                id,
                format!("both HERDR_ENV and TMUX are set; the nearest parent is {id}"),
            ),
            None => (
                first,
                "both HERDR_ENV and TMUX are set; no parent is tmux or herdr, so herdr first"
                    .to_owned(),
            ),
        },
    };
    HostPick { id, why }
}

/// Each of these overrides the stored Claude login for every member and
/// changes who is billed.
const CREDENTIAL_VARS: [&str; 5] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
];

/// What the checks look at.
pub(crate) struct Inputs<'a> {
    pub(crate) host: Host,
    pub(crate) home: PathBuf,
    pub(crate) platform: &'a dyn Platform,
    /// The adapters and the clock `status` reads members with.
    pub(crate) registry: &'a Registry,
    pub(crate) clock: &'a dyn Clock,
    /// `$PATH`, where the harness and host binaries are looked for.
    pub(crate) path_var: Option<OsString>,
    /// Where Claude Code keeps `settings.json`, the skills and `.claude.json`.
    pub(crate) claude: ClaudeConfig,
    /// The Codex adapter, whose plugin is checked when `codex` is on PATH.
    pub(crate) codex: CodexSetup,
    /// The pi adapter: its directory holds herdr's pi integration, and its
    /// lead package is checked when `pi` is on PATH.
    pub(crate) pi: PiSetup,
    /// The MCP entry `teamwire install` would register from this binary, or
    /// why it cannot be worked out.
    pub(crate) lead_entry: Result<McpEntry, String>,
    /// True when this binary is a cargo build under `target/`.
    pub(crate) running_a_build: bool,
    /// The directory `.worktrees/` is checked in.
    pub(crate) repo: PathBuf,
    /// The names of the environment variables that are set, never their
    /// values (SEC-1).
    pub(crate) env_names: Vec<String>,
}

/// The nearest directory at or above `path` that exists, so a state
/// directory not created yet still has a disk to ask about.
fn existing_ancestor(path: &Path) -> &Path {
    path.ancestors().find(|p| p.exists()).unwrap_or(path)
}

fn disk_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let at = existing_ancestor(&inputs.home);
    let free = match inputs.platform.free_bytes(at) {
        Ok(bytes) if bytes >= MIN_FREE_BYTES => {
            Check::pass("free disk", format!("{} MB free", bytes / (1024 * 1024)))
        }
        Ok(bytes) => Check::fail(
            "free disk",
            format!(
                "{} MB free; launch needs {} MB",
                bytes / (1024 * 1024),
                MIN_FREE_BYTES / (1024 * 1024)
            ),
        ),
        Err(e) => Check::warn("free disk", format!("could not read free space: {e}")),
    };
    let network = match inputs.platform.on_network_fs(at) {
        Ok(false) => Check::pass(
            "network filesystem",
            "the state directory is on a local disk",
        ),
        Ok(true) => Check::warn(
            "network filesystem",
            format!(
                "{} is on a network filesystem, where SQLite locking cannot be trusted; \
                 set TEAMWIRE_HOME to a local disk",
                inputs.home.display()
            ),
        ),
        Err(e) => Check::warn(
            "network filesystem",
            format!("could not tell which filesystem holds the state directory: {e}"),
        ),
    };
    vec![free, network]
}

/// The database checks of one team: opens, `quick_check`, the close lock and
/// the watch lease.
fn team_checks(inputs: &Inputs<'_>, team: &TeamName) -> Vec<Check> {
    let name = |what: &str| format!("team {team} {what}");
    let mut store = match Store::open_read_only(&inputs.home, team) {
        Ok(store) => store,
        Err(e) => return vec![Check::fail(name("database"), e.to_string())],
    };
    let mut checks = Vec::new();
    match store.schema_version() {
        Ok(v) if v == SCHEMA_VERSION => {
            checks.push(Check::pass(name("schema"), format!("version {v}")));
        }
        // A read-only open refuses a newer schema, so this is an older one.
        // Reads work on it, and the next command that writes migrates it.
        Ok(v) => checks.push(Check::pass(
            name("schema"),
            format!("version {v}; the next write migrates it to {SCHEMA_VERSION}"),
        )),
        Err(e) => checks.push(Check::fail(name("schema"), e.to_string())),
    }
    match store.quick_check() {
        Ok(text) if text == "ok" => checks.push(Check::pass(name("quick_check"), "ok")),
        Ok(text) => checks.push(Check::fail(
            name("quick_check"),
            format!("the database has problems: {text}"),
        )),
        Err(e) => checks.push(Check::fail(name("quick_check"), e.to_string())),
    }
    let rows = store.read(|tx| Ok((tx.team()?, tx.watch_lease()?)));
    match rows {
        Ok((team_row, lease)) => {
            if team_row
                .as_ref()
                .is_some_and(|r| r.state == TeamState::Open)
            {
                checks.push(older_members_check(inputs, &mut store, team));
            }
            // The pid with its recorded start time: a pid that a later
            // process reused is gone, as `close --clear-stale-lock` judges it.
            if let Some(row) = team_row
                && row.state == TeamState::Closing
                && let Some(pid) = row.closing_pid
                && !inputs.platform.process_alive(ProcessRef {
                    pid,
                    started: row.closing_started,
                })
            {
                checks.push(Check::fail(
                    name("close lock"),
                    format!(
                        "held by pid {pid}, which is gone; run \
                         `teamwire close {team} --clear-stale-lock`"
                    ),
                ));
            }
            checks.push(counters_check(&mut store, team));
            if let Some(lease) = lease
                && teamwire_deliver::watch_lease_is_dead(
                    &lease,
                    inputs.platform,
                    inputs.clock.now(),
                )
            {
                // The same rule as `status`: a live pid with an old heartbeat
                // is a pid the system gave to another process.
                let why = if inputs.platform.pid_alive(lease.pid) {
                    "is alive but has not beaten for over 600 s"
                } else {
                    "is gone"
                };
                checks.push(Check::warn(
                    name("watch lease"),
                    format!(
                        "held by pid {}, which {why}; the next `teamwire watch {team}` takes it over",
                        lease.pid
                    ),
                ));
            }
        }
        Err(e) => checks.push(Check::fail(name("rows"), e.to_string())),
    }
    checks
}

/// A message counter below the largest value in use makes every later send
/// collide. Doctor only prints the repair, so the user decides to run it.
fn counters_check(store: &mut Store, team: &TeamName) -> Check {
    let name = format!("team {team} counters");
    let found = store.read(|tx| Ok((tx.counter_floors()?, tx.counters()?.all_or_corrupt()?)));
    let (floors, counters) = match found {
        Ok(found) => found,
        Err(e) => return Check::fail(name, e.to_string()),
    };
    let damaged: Vec<String> = floors
        .iter()
        .filter_map(|(counter, used)| {
            let stored = counters.iter().find(|(n, _)| n == counter).map(|c| c.1);
            (stored.unwrap_or(0) < *used).then(|| {
                let now = stored.map_or_else(|| "missing".to_owned(), |v| v.to_string());
                // An upsert, because a missing row is also damage and an
                // UPDATE of it would change nothing.
                format!(
                    "counter {counter} is {now} but {used} is in use; repair with: \
                     INSERT INTO counters (name, value) VALUES ('{counter}', {used}) \
                     ON CONFLICT (name) DO UPDATE SET value = excluded.value;"
                )
            })
        })
        .collect();
    if damaged.is_empty() {
        return Check::pass(name, "every message counter is at or above its largest id");
    }
    Check::fail(name, damaged.join(" "))
}

/// The members `status` marks as started by another build of teamwire. The
/// check reuses the status view, so doctor and `status` cannot disagree.
fn older_members_check(inputs: &Inputs<'_>, store: &mut Store, team: &TeamName) -> Check {
    let name = format!("team {team} older members");
    let config = Config::default();
    let ctx = Ctx {
        home: &inputs.home,
        store,
        registry: inputs.registry,
        platform: inputs.platform,
        clock: inputs.clock,
        config: &config,
    };
    let view = teamwire_watch::status_with(&ctx, None);
    if !view.errors.is_empty() {
        return Check::warn(
            name,
            format!("could not read the members: {}", view.errors.join("; ")),
        );
    }
    let older: Vec<String> = view
        .members
        .iter()
        // A member whose result is settled has finished, so relaunching it
        // changes nothing (the rule of the lead's Stop hook). A result still
        // pending or delivering is what the watch's UNREPORTED line is about,
        // so that member stays listed.
        .filter(|m| {
            m.older_build
                && m.result
                    .is_none_or(|r| matches!(r, ReportState::Pending | ReportState::Delivering))
        })
        .map(|m| format!("{} ({})", m.role, m.started_by.as_deref().unwrap_or("?")))
        .collect();
    if older.is_empty() {
        return Check::pass(name, "no member was started by another build");
    }
    Check::warn(
        name,
        format!(
            "{} started by a build other than {}; relaunch the member, or ignore if its \
             wiring did not change. A member started before builds recorded their \
             commit shows as 0.0.0",
            older.join(", "),
            teamwire_store::WRITER_VERSION
        ),
    )
}

fn database_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let teams = match teamwire_store::team_names(&inputs.home) {
        Ok(teams) => teams,
        Err(e) => return vec![Check::fail("databases", e.to_string())],
    };
    if teams.is_empty() {
        return vec![Check::pass(
            "databases",
            format!("no teams under {}", inputs.home.display()),
        )];
    }
    teams.iter().flat_map(|t| team_checks(inputs, t)).collect()
}

/// The hook herdr manages reports a session's state to herdr. It is a
/// command in Claude Code's `settings.json` that names herdr.
fn herdr_hook_check(inputs: &Inputs<'_>) -> Check {
    let path = inputs.claude.claude_dir.join("settings.json");
    match std::fs::read_to_string(&path) {
        Ok(text) if text.contains("herdr") => {
            Check::pass("herdr session hook", format!("found in {}", path.display()))
        }
        Ok(_) => Check::fail(
            "herdr session hook",
            format!("{} has no hook that names herdr", path.display()),
        ),
        Err(e) => Check::fail(
            "herdr session hook",
            format!("could not read {}: {e}", path.display()),
        ),
    }
}

/// herdr learns a pi session from its own pi extension, and delivery to a pi
/// member under herdr needs that. Checked only when `pi` is on PATH.
fn herdr_pi_check(inputs: &Inputs<'_>) -> Option<Check> {
    on_path(inputs.path_var.as_ref(), "pi")?;
    let file = inputs
        .pi
        .config()
        .agent_dir
        .join("extensions/herdr-agent-state.ts");
    let name = "herdr pi integration";
    Some(if file.is_file() {
        Check::pass(name, format!("found at {}", file.display()))
    } else {
        Check::warn(
            name,
            format!(
                "{} is missing, so herdr cannot find a pi member's session; run `herdr integration install pi`",
                file.display()
            ),
        )
    })
}

/// `git check-ignore` exits 0 for an ignored path, 1 for a path that is not
/// ignored, and 128 outside a repository.
fn worktrees_check(inputs: &Inputs<'_>) -> Check {
    let cmd = Command::new("git")
        .args(["check-ignore", "-q", ".worktrees/probe"])
        .cwd(inputs.repo.clone());
    match inputs.platform.run(&cmd, GIT_TIMEOUT) {
        Ok(out) if out.exit_code == 0 => {
            Check::pass(".worktrees ignored", "git ignores .worktrees/ here")
        }
        Ok(out) if out.exit_code == 1 => Check::fail(
            ".worktrees ignored",
            "git does not ignore .worktrees/; add it to .gitignore or .git/info/exclude",
        ),
        Ok(_) => Check::pass(".worktrees ignored", "not inside a git repository"),
        Err(e) => Check::warn(".worktrees ignored", format!("could not run git: {e}")),
    }
}

fn on_path(path_var: Option<&OsString>, program: &str) -> Option<PathBuf> {
    let dirs = std::env::split_paths(path_var?);
    dirs.map(|d| d.join(program)).find(|p| p.is_file())
}

/// Runs `program args` and returns its output, or why it gave none. A probe
/// that fails or times out is a warning for the caller, never a hang.
fn probe(inputs: &Inputs<'_>, program: &Path, args: &[&str]) -> Result<String, String> {
    let cmd = Command::new(program.to_string_lossy().into_owned()).args(args.iter().copied());
    let out = inputs
        .platform
        .run(&cmd, PROBE_TIMEOUT)
        .map_err(|e| e.to_string())?;
    if out.timed_out {
        return Err(format!("no answer within {} s", PROBE_TIMEOUT.as_secs()));
    }
    if out.exit_code != 0 {
        return Err(format!("exited with code {}", out.exit_code));
    }
    // Some tools print their help on stderr.
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(text)
}

/// The first line of a `--version` answer, which is the version itself.
fn version_check(inputs: &Inputs<'_>, program: &str, path: &Path, flag: &str) -> Check {
    let name = format!("{program} version");
    match probe(inputs, path, &[flag]) {
        Ok(text) => Check::pass(name, text.lines().next().unwrap_or("").trim().to_owned()),
        Err(e) => Check::warn(name, format!("`{program} {flag}` failed: {e}")),
    }
}

/// Claude Code writes a flag as `--name-suffix` or, in a description, as
/// `--name[-suffix]` (2.1.301 does this for `--append-system-prompt-file`).
fn lists_flag(help: &str, flag: &str) -> bool {
    help.contains(flag)
        || flag
            .rsplit_once('-')
            .is_some_and(|(base, suffix)| help.contains(&format!("{base}[-{suffix}]")))
}

fn claude_flag_check(inputs: &Inputs<'_>, path: &Path) -> Check {
    let name = "claude flags";
    match probe(inputs, path, &["--help"]) {
        Ok(help) => {
            let missing: Vec<&str> = CLAUDE_FLAGS
                .into_iter()
                .filter(|f| !lists_flag(&help, f))
                .collect();
            if missing.is_empty() {
                Check::pass(name, "the help text lists every flag teamwire passes")
            } else {
                Check::warn(
                    name,
                    format!(
                        "`claude --help` does not list {}; teamwire passes them when it \
                         starts a member, so this claude may be too old or too new",
                        missing.join(", ")
                    ),
                )
            }
        }
        Err(e) => Check::warn(name, format!("`claude --help` failed: {e}")),
    }
}

fn binary_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let mut checks = Vec::new();
    let host_program = match inputs.host {
        Host::Herdr => Some("herdr"),
        Host::Tmux => Some("tmux"),
        Host::None => None,
    };
    let wanted = std::iter::once(("claude", "harness")).chain(host_program.map(|h| (h, "host")));
    for (program, role) in wanted {
        let name = format!("{program} on PATH");
        match on_path(inputs.path_var.as_ref(), program) {
            Some(path) => {
                checks.push(Check::pass(
                    name,
                    format!("{role} binary at {}", path.display()),
                ));
                if program == "claude" {
                    checks.push(version_check(inputs, program, &path, "--version"));
                    checks.push(claude_flag_check(inputs, &path));
                } else {
                    let flag = if program == "tmux" { "-V" } else { "--version" };
                    checks.push(version_check(inputs, program, &path, flag));
                }
            }
            None => checks.push(Check::fail(
                name,
                format!("the {role} binary `{program}` is not on PATH"),
            )),
        }
    }
    // Codex is optional: without it on PATH, doctor says nothing about it.
    if let Some(path) = on_path(inputs.path_var.as_ref(), "codex") {
        checks.push(Check::pass(
            "codex on PATH",
            format!("harness binary at {}", path.display()),
        ));
        checks.push(version_check(inputs, "codex", &path, "--version"));
    }
    if inputs.host == Host::None {
        checks.push(Check::pass(
            "host",
            "no herdr pane or tmux session: members cannot be started from here (host none)",
        ));
    }
    checks
}

fn credential_check(inputs: &Inputs<'_>) -> Check {
    let set: Vec<&str> = CREDENTIAL_VARS
        .into_iter()
        .filter(|v| inputs.env_names.iter().any(|n| n == v))
        .collect();
    if set.is_empty() {
        return Check::pass(
            "credential variables",
            "none set; members use the stored login",
        );
    }
    Check::warn(
        "credential variables",
        format!(
            "{} set in this environment; each overrides the stored login for every \
             member and changes who is billed. Unset it unless that is intended",
            set.join(", ")
        ),
    )
}

/// The lead setup's checks, from `lead_setup_check`.
fn lead_setup_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let lead = lead_setup_check::LeadInputs {
        claude: &inputs.claude,
        entry: &inputs.lead_entry,
        running_a_build: inputs.running_a_build,
        dir: &inputs.repo,
        codex: on_path(inputs.path_var.as_ref(), "codex").map(|_| &inputs.codex),
        pi: on_path(inputs.path_var.as_ref(), "pi").map(|_| &inputs.pi),
        home: &inputs.home,
    };
    lead_setup_check::run(&lead)
        .into_iter()
        .map(|(name, outcome)| match outcome {
            Outcome::Pass(detail) => Check::pass(name, detail),
            Outcome::Warn(detail) => Check::warn(name, detail),
            Outcome::Fail(detail) => Check::fail(name, detail),
        })
        .collect()
}

/// Every check, in the order `doctor` prints them.
pub(crate) fn diagnose(inputs: &Inputs<'_>) -> Vec<Check> {
    let mut checks = disk_checks(inputs);
    checks.extend(database_checks(inputs));
    if inputs.host == Host::Herdr {
        checks.push(herdr_hook_check(inputs));
        checks.extend(herdr_pi_check(inputs));
    }
    checks.push(worktrees_check(inputs));
    checks.extend(binary_checks(inputs));
    checks.push(credential_check(inputs));
    checks.extend(lead_setup_checks(inputs));
    checks
}

/// 0 when every check passed, else 2.
pub(crate) fn exit_code(checks: &[Check]) -> u8 {
    if checks.iter().all(|c| c.level == Level::Pass) {
        0
    } else {
        PARTIAL
    }
}

pub(crate) fn render(checks: &[Check]) -> String {
    checks.iter().map(|c| c.line() + "\n").collect()
}

pub(crate) fn run(global: &Global, wiring: &Wiring) -> ExitCode {
    let home = match home_or_refuse("doctor", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let lead_entry = wanted_entry(global);
    let pick = pick_host(
        std::env::var("HERDR_ENV").ok().as_deref(),
        std::env::var("TMUX").ok().as_deref(),
        &|pid| wiring.platform.parent_of(pid),
    );
    let inputs = Inputs {
        host: Host::from_id(pick.id),
        home,
        platform: wiring.platform.as_ref(),
        registry: &wiring.registry,
        clock: &wiring.clock,
        path_var: std::env::var_os("PATH"),
        claude: wiring.claude.clone(),
        codex: wiring.codex_setup(),
        pi: wiring.pi_setup(),
        running_a_build: lead_entry
            .as_ref()
            .is_ok_and(|e| lead_setup_check::is_build_output(Path::new(&e.command))),
        lead_entry,
        repo,
        env_names: std::env::vars_os()
            .filter(|(_, v)| !v.is_empty())
            .filter_map(|(k, _)| k.into_string().ok())
            .collect(),
    };
    let mut checks = diagnose(&inputs);
    // With no host, `binary_checks` already says so.
    if inputs.host != Host::None {
        checks.insert(
            0,
            Check::pass("host", format!("{} ({})", pick.id, pick.why)),
        );
    }
    say(&render(&checks));
    ExitCode::from(exit_code(&checks))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A parent chain from a list of (pid, program), each the parent of the one before.
    fn chain(programs: &'static [&'static str]) -> impl Fn(Pid) -> Option<(Pid, String)> + 'static {
        let me = std::process::id();
        move |pid| {
            let step = usize::try_from(pid.get().checked_sub(me)?).ok()?;
            let name = programs.get(step)?;
            Some((Pid::new(pid.get() + 1), (*name).to_owned()))
        }
    }

    const BOTH: (Option<&str>, Option<&str>) = (Some("1"), Some("/tmp/tmux-1/default,42,0"));

    #[test]
    fn with_both_variables_the_nearest_parent_decides() {
        let pick = |programs| pick_host(BOTH.0, BOTH.1, &chain(programs)).id;
        assert_eq!(pick(&["teamwire", "zsh", "tmux", "zsh", "herdr"]), "tmux");
        assert_eq!(pick(&["teamwire", "zsh", "herdr", "zsh", "tmux"]), "herdr");
        assert_eq!(pick(&["teamwire", "tmux: server"]), "tmux");
        // Neither found, or the first lookup fails: herdr first, as before.
        assert_eq!(pick(&["teamwire", "zsh", "launchd"]), "herdr");
        assert_eq!(pick(&[]), "herdr");
        assert_eq!(
            pick_host(BOTH.0, BOTH.1, &chain(&["teamwire", "tmux"])).why,
            "both HERDR_ENV and TMUX are set; the nearest parent is tmux"
        );
    }

    #[test]
    fn a_loop_in_the_parents_stops_after_64_steps() {
        let calls = std::cell::Cell::new(0);
        let looping = |pid: Pid| {
            calls.set(calls.get() + 1);
            Some((pid, "zsh".to_owned()))
        };
        assert_eq!(nearest_host(Pid::new(7), &looping), None);
        assert_eq!(calls.get(), MAX_PARENT_STEPS);
    }

    #[test]
    fn one_variable_decides_without_looking_at_parents() {
        let calls = std::cell::Cell::new(0);
        let lookup = |_: Pid| {
            calls.set(calls.get() + 1);
            None
        };
        assert_eq!(pick_host(Some("1"), None, &lookup).id, "herdr");
        assert_eq!(pick_host(None, Some("/tmp/t,1,0"), &lookup).id, "tmux");
        assert_eq!(calls.get(), 0);
    }
}
