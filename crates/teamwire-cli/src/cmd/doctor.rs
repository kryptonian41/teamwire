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

use teamwire_core::{CheckOutcome, Clock, Command, LeadCommand, LeadTarget, Platform, Registry};

use crate::Global;
use crate::cmd::doctor_teams::database_checks;
pub(crate) use crate::cmd::host_pick::{Host, HostPick, pick_host};
use crate::cmd::install::wanted_command;
use crate::cmd::lead_setup_check::{self, HarnessInputs, on_path, version_check};
use crate::cmd::shared::{PARTIAL, home_or_refuse, say};
use crate::wiring::{ClaudeConfig, PiConfig, SKILL_TEXT, Wiring};

/// Launch refuses to start below this much free space (LCH-2).
const MIN_FREE_BYTES: u64 = 200 * 1024 * 1024;

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Each probe of a harness or host binary waits this long, then warns.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

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

    pub(crate) fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Level::Pass, name, detail)
    }

    pub(crate) fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Level::Warn, name, detail)
    }

    pub(crate) fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Level::Fail, name, detail)
    }

    fn of(name: impl Into<String>, outcome: CheckOutcome) -> Self {
        match outcome {
            CheckOutcome::Pass(detail) => Self::pass(name, detail),
            CheckOutcome::Warn(detail) => Self::warn(name, detail),
            CheckOutcome::Fail(detail) => Self::fail(name, detail),
        }
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
    /// The adapters and the clock `status` reads members with. Each
    /// harness's program and lead setup are checked through it too.
    pub(crate) registry: &'a Registry,
    pub(crate) clock: &'a dyn Clock,
    /// `$PATH`, where the harness and host binaries are looked for.
    pub(crate) path_var: Option<OsString>,
    /// Where Claude Code keeps `settings.json`, which holds herdr's hook.
    pub(crate) claude: ClaudeConfig,
    /// Where pi keeps its files, which hold herdr's pi integration.
    pub(crate) pi: PiConfig,
    /// The command `teamwire install` would give each lead setup from this
    /// binary, or why it cannot be worked out.
    pub(crate) lead_command: Result<LeadCommand, String>,
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
    on_path(inputs.path_var.as_ref(), &inputs.pi.pi_bin)?;
    let file = inputs.pi.agent_dir.join("extensions/herdr-agent-state.ts");
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

/// The harness checks of `lead_setup_check`, which loop over the registry.
fn harness_checks(
    inputs: &Inputs<'_>,
    checks: impl FnOnce(&HarnessInputs<'_>) -> Vec<(String, CheckOutcome)>,
) -> Vec<Check> {
    let probe = |path: &Path, args: &[&str]| probe(inputs, path, args);
    let target = match &inputs.lead_command {
        Ok(command) => Ok(LeadTarget {
            command,
            home: &inputs.home,
            shared_skill: SKILL_TEXT,
            compare_files: !inputs.running_a_build,
        }),
        Err(e) => Err(e.clone()),
    };
    checks(&HarnessInputs {
        registry: inputs.registry,
        path_var: inputs.path_var.as_ref(),
        probe: &probe,
        target,
        dir: &inputs.repo,
    })
    .into_iter()
    .map(|(name, outcome)| Check::of(name, outcome))
    .collect()
}

/// The default lead harness's program, then the host's, then every other
/// harness's that is on PATH.
fn binary_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let mut checks = harness_checks(inputs, |h| lead_setup_check::program_checks(h, true));
    let host_program = match inputs.host {
        Host::Herdr => Some("herdr"),
        Host::Tmux => Some("tmux"),
        Host::None => None,
    };
    if let Some(program) = host_program {
        let name = format!("{program} on PATH");
        match on_path(inputs.path_var.as_ref(), program) {
            Some(path) => {
                checks.push(Check::pass(
                    name,
                    format!("host binary at {}", path.display()),
                ));
                let flag = if program == "tmux" { "-V" } else { "--version" };
                let probe = |path: &Path, args: &[&str]| probe(inputs, path, args);
                let (name, outcome) = version_check(&probe, program, &path, flag);
                checks.push(Check::of(name, outcome));
            }
            None => checks.push(Check::fail(
                name,
                format!("the host binary `{program}` is not on PATH"),
            )),
        }
    }
    // Every other harness is optional: without its program on PATH, doctor
    // says nothing about it.
    checks.extend(harness_checks(inputs, |h| {
        lead_setup_check::program_checks(h, false)
    }));
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
    checks.extend(harness_checks(inputs, lead_setup_check::lead_checks));
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

/// The checks' inputs for this process, and the host pick they rest on.
pub(crate) fn inputs<'w>(
    home: PathBuf,
    global: &Global,
    wiring: &'w Wiring,
) -> (Inputs<'w>, HostPick) {
    let lead_command = wanted_command(global);
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
        pi: wiring.pi.clone(),
        running_a_build: lead_command
            .as_ref()
            .is_ok_and(|c| lead_setup_check::is_build_output(&c.bin)),
        lead_command,
        repo: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        env_names: std::env::vars_os()
            .filter(|(_, v)| !v.is_empty())
            .filter_map(|(k, _)| k.into_string().ok())
            .collect(),
    };
    (inputs, pick)
}

pub(crate) fn run(global: &Global, wiring: &Wiring) -> ExitCode {
    let home = match home_or_refuse("doctor", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let (inputs, pick) = inputs(home, global, wiring);
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
    use crate::cmd::host_pick::{MAX_PARENT_STEPS, nearest_host};
    use teamwire_core::Pid;

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
