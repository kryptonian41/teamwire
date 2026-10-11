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
use teamwire_core::{Command, Platform, TeamName, TeamState};
use teamwire_store::{SCHEMA_VERSION, Store};

use crate::Global;
use crate::cmd::install::wanted_entry;
use crate::cmd::lead_setup_check::{self, Outcome};
use crate::cmd::send::home_or_refuse;
use crate::wiring::{ClaudeConfig, McpEntry, Wiring};

/// Exit code of a doctor run that found a problem (`docs/api/cli.md`).
const PARTIAL: u8 = 2;

/// Launch refuses to start below this much free space (LCH-2).
const MIN_FREE_BYTES: u64 = 200 * 1024 * 1024;

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

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

/// What the checks look at.
pub(crate) struct Inputs<'a> {
    pub(crate) host: Host,
    pub(crate) home: PathBuf,
    pub(crate) platform: &'a dyn Platform,
    /// `$PATH`, where the harness and host binaries are looked for.
    pub(crate) path_var: Option<OsString>,
    /// Where Claude Code keeps `settings.json`, the skills and `.claude.json`.
    pub(crate) claude: ClaudeConfig,
    /// The MCP entry `teamwire install` would register from this binary, or
    /// why it cannot be worked out.
    pub(crate) lead_entry: Result<McpEntry, String>,
    /// True when this binary is a cargo build under `target/`.
    pub(crate) running_a_build: bool,
    /// The directory `.worktrees/` is checked in.
    pub(crate) repo: PathBuf,
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

/// Every directory under `<home>/teams` that names a team and holds a database.
fn team_names(home: &Path) -> Vec<TeamName> {
    let Ok(entries) = std::fs::read_dir(home.join("teams")) else {
        return Vec::new();
    };
    let mut names: Vec<TeamName> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().and_then(|n| TeamName::parse(n).ok()))
        .filter(|n| teamwire_store::database_path(home, n).is_file())
        .collect();
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    names
}

/// The database checks of one team: opens, `quick_check`, the close lock and
/// the watch lease.
fn team_checks(inputs: &Inputs<'_>, team: &TeamName) -> Vec<Check> {
    let name = |what: &str| format!("team {team} {what}");
    let store = match Store::open_read_only(&inputs.home, team) {
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
            if let Some(lease) = lease
                && !inputs.platform.pid_alive(lease.pid)
            {
                checks.push(Check::warn(
                    name("watch lease"),
                    format!(
                        "held by pid {}, which is gone; the next `teamwire watch {team}` takes it over",
                        lease.pid
                    ),
                ));
            }
        }
        Err(e) => checks.push(Check::fail(name("rows"), e.to_string())),
    }
    checks
}

fn database_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let teams = team_names(&inputs.home);
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
            Some(path) => checks.push(Check::pass(
                name,
                format!("{role} binary at {}", path.display()),
            )),
            None => checks.push(Check::fail(
                name,
                format!("the {role} binary `{program}` is not on PATH"),
            )),
        }
    }
    if inputs.host == Host::None {
        checks.push(Check::pass(
            "host",
            "no herdr pane or tmux session: members cannot be started from here (host none)",
        ));
    }
    checks
}

/// The lead setup's checks, from `lead_setup_check`.
fn lead_setup_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let lead = lead_setup_check::LeadInputs {
        claude: &inputs.claude,
        entry: &inputs.lead_entry,
        running_a_build: inputs.running_a_build,
        dir: &inputs.repo,
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
    }
    checks.push(worktrees_check(inputs));
    checks.extend(binary_checks(inputs));
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

#[expect(clippy::print_stdout, reason = "the report is the command's output")]
fn say(text: &str) {
    print!("{text}");
}

pub(crate) fn run(global: &Global, wiring: &Wiring) -> ExitCode {
    let home = match home_or_refuse("doctor", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let lead_entry = wanted_entry(global);
    let inputs = Inputs {
        host: Host::detect(
            std::env::var("HERDR_ENV").ok().as_deref(),
            std::env::var("TMUX").ok().as_deref(),
        ),
        home,
        platform: wiring.platform.as_ref(),
        path_var: std::env::var_os("PATH"),
        claude: wiring.claude.clone(),
        running_a_build: lead_entry
            .as_ref()
            .is_ok_and(|e| lead_setup_check::is_build_output(Path::new(&e.command))),
        lead_entry,
        repo,
    };
    let checks = diagnose(&inputs);
    say(&render(&checks));
    ExitCode::from(exit_code(&checks))
}
