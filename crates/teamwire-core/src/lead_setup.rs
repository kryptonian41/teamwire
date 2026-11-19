//! The `LeadSetup` trait: what `teamwire install`, `uninstall` and `doctor`
//! ask of a harness, so the CLI loops over the registered harnesses and never
//! names one (ADR 0031). Each adapter keeps its own mechanism (a Claude Code
//! plugin, a pi package). This module holds what they share:
//! the command the setup runs, the change list the CLI only displays, the one
//! loop that makes changes in order, and the errors.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

use crate::platform::{Command, Platform};

/// How long one command of a harness's own program may take while a lead
/// setup is planned or changed.
pub const LEAD_SETUP_TIMEOUT: Duration = Duration::from_secs(30);

/// The program a lead setup runs: `<bin> [--home DIR]`, then `mcp --lead`,
/// a hook or `watch`. Each adapter renders its own entry from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadCommand {
    pub bin: PathBuf,
    /// Given only when the state directory is not the default, so a default
    /// install keeps following the default.
    pub home: Option<PathBuf>,
}

impl LeadCommand {
    /// The home as rendered: rebuilt from its components, so `/x/` and `/x`
    /// give one text and a second install does not see a different setup.
    #[must_use]
    pub fn home_text(&self) -> Option<String> {
        self.home
            .as_ref()
            .map(|h| h.components().collect::<PathBuf>().display().to_string())
    }

    /// `<bin>`, then `--home DIR` when a home is given.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        let mut words = vec![self.bin.display().to_string()];
        if let Some(home) = self.home_text() {
            words.extend(["--home".to_owned(), home]);
        }
        words
    }
}

/// What a lead setup is planned and checked against.
#[derive(Debug, Clone, Copy)]
pub struct LeadTarget<'a> {
    pub command: &'a LeadCommand,
    /// The state directory, which holds each harness's setup files.
    pub home: &'a Path,
    /// The shared skill text. The CLI passes it in, because one adapter crate
    /// may not depend on another (ADR 0023 point 9).
    pub shared_skill: &'a str,
    /// False for a cargo build under `target/`, whose paths and text differ
    /// from the installed binary's by design: then only whether the setup is
    /// there and switched on counts.
    pub compare_files: bool,
}

/// What a harness has of the lead setup, compared with what this binary
/// would install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeadStatus {
    Missing,
    /// There, but another version, switched off, or from another source:
    /// each problem in words.
    Stale(Vec<String>),
    Current,
}

/// What one change does when it is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeadStep {
    /// Write files through the platform: each path is relative to `dir`.
    WriteFiles {
        dir: PathBuf,
        files: Vec<(PathBuf, String)>,
    },
    /// Delete a directory teamwire wrote.
    RemoveDir(PathBuf),
    /// Run the harness's own program; any exit but 0 fails the change.
    Run(Command),
    /// A step only the adapter knows how to make, on this path.
    Own(PathBuf),
}

/// One change install or uninstall makes. The CLI only prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadChange {
    text: String,
    step: LeadStep,
}

impl LeadChange {
    /// `text` says what the change does, after "would" or "did".
    pub fn new(text: impl Into<String>, step: LeadStep) -> Self {
        Self {
            text: text.into(),
            step,
        }
    }

    pub fn step(&self) -> &LeadStep {
        &self.step
    }
}

impl fmt::Display for LeadChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// A harness's planned changes, and what to tell the user once they are
/// made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LeadPlan {
    pub changes: Vec<LeadChange>,
    pub note: Option<&'static str>,
}

/// Why a lead setup could not be read or planned. Nothing was written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LeadSetupError {
    #[error("cannot read {}: {detail}", path.display())]
    Read { path: PathBuf, detail: String },
    /// A check failed before anything was written.
    #[error("{0}")]
    Refused(String),
}

impl LeadSetupError {
    /// A refusal with the way past it: `--force` to `verb` it.
    pub fn needs_force(what: &str, verb: &str) -> Self {
        Self::Refused(format!("{what}; pass --force to {verb} it"))
    }
}

/// A change that failed after the checks passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyError<C = LeadChange> {
    /// The changes made before the failure, in order.
    pub done: Vec<C>,
    pub failed: C,
    pub detail: String,
}

impl<C: fmt::Display> fmt::Display for ApplyError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed: {}", self.failed, self.detail)
    }
}

impl<C: fmt::Display + fmt::Debug> std::error::Error for ApplyError<C> {}

/// One doctor check's verdict and its detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Pass(String),
    Warn(String),
    Fail(String),
}

/// A doctor check: its name and verdict.
pub type DoctorCheck = (&'static str, CheckOutcome);

/// Runs a harness's program with the given arguments through doctor's
/// platform: its output, or why it gave none.
pub type Probe<'a> = &'a dyn Fn(&[&str]) -> Result<String, String>;

/// One harness's lead setup. `Harness::lead_setup` hands it out; the CLI
/// reaches every one through the `Registry`.
pub trait LeadSetup: Send + Sync {
    /// The program the harness runs, which install and doctor look for.
    fn program(&self) -> &str;
    /// The harness's name in a sentence, such as "Claude Code".
    fn label(&self) -> &'static str;
    /// The harness teamwire assumes when nothing names one: doctor fails
    /// without its program, install without a terminal or `--harness`
    /// installs it, and uninstall always plans it and stops at its refusal.
    /// Every other harness is optional: uninstall plans it only when its
    /// program runs, and skips it on a refusal. Default `false`.
    fn default_lead(&self) -> bool {
        false
    }
    /// What the harness has, against `target`. Reads only.
    fn status(&self, target: &LeadTarget<'_>) -> Result<LeadStatus, LeadSetupError>;
    /// The changes that make the setup this binary's, after every check.
    /// Nothing is written. `force` replaces what another source registered.
    fn plan_install(
        &self,
        target: &LeadTarget<'_>,
        force: bool,
    ) -> Result<LeadPlan, LeadSetupError>;
    /// The changes that remove the setup kept under `home`, after every
    /// check. Nothing is written.
    fn plan_uninstall(&self, home: &Path, force: bool) -> Result<LeadPlan, LeadSetupError>;
    /// Makes `changes`, which this harness planned, in order, stopping at the
    /// first failure.
    fn apply(&self, changes: &[LeadChange]) -> Result<(), Box<ApplyError>>;
    /// Doctor's checks of the setup. `target` is an error when the command
    /// cannot be worked out; `dir` is the folder doctor runs in. Reads only.
    fn doctor_checks(&self, target: Result<&LeadTarget<'_>, &str>, dir: &Path) -> Vec<DoctorCheck>;
    /// Doctor's checks of the program beyond its version, each run through
    /// `probe`. Default none.
    fn program_checks(&self, _probe: Probe<'_>) -> Vec<DoctorCheck> {
        Vec::new()
    }
}

/// The doctor check of an optional harness's setup: missing passes and names
/// the command that installs it, stale and unreadable warn.
pub fn optional_check(
    status: Result<LeadStatus, LeadSetupError>,
    harness_id: &str,
    label: &str,
) -> CheckOutcome {
    let add = format!("run `teamwire install --harness {harness_id}`");
    match status {
        Err(e) => CheckOutcome::Warn(e.to_string()),
        Ok(LeadStatus::Missing) => {
            CheckOutcome::Pass(format!("not installed; to lead from {label}, {add}"))
        }
        Ok(LeadStatus::Stale(problems)) => {
            CheckOutcome::Warn(format!("{}; {add}", problems.join("; ")))
        }
        Ok(LeadStatus::Current) => CheckOutcome::Pass("installed".to_owned()),
    }
}

/// Makes `changes` in order with `one`, stopping at the first failure: the
/// loop every adapter's apply runs.
pub fn apply_in_order<C: Clone>(
    changes: &[C],
    mut one: impl FnMut(&C) -> Result<(), String>,
) -> Result<(), Box<ApplyError<C>>> {
    let mut done = Vec::new();
    for change in changes {
        if let Err(detail) = one(change) {
            return Err(Box::new(ApplyError {
                done,
                failed: change.clone(),
                detail,
            }));
        }
        done.push(change.clone());
    }
    Ok(())
}

/// Makes one shared step through `platform`. An `Own` step belongs to the
/// adapter, which makes it before calling this; here it is refused.
pub fn make_step(platform: &dyn Platform, step: &LeadStep) -> Result<(), String> {
    match step {
        LeadStep::WriteFiles { dir, files } => {
            for (rel, text) in files {
                let path = dir.join(rel);
                if let Some(parent) = path.parent() {
                    platform.create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                platform
                    .write_atomic(&path, text.as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        LeadStep::RemoveDir(dir) => platform.remove_dir_all(dir).map_err(|e| e.to_string()),
        LeadStep::Run(cmd) => run_program(platform, cmd).map(drop),
        LeadStep::Own(path) => Err(format!(
            "no step of this harness acts on {}",
            path.display()
        )),
    }
}

/// Runs a harness's program under `LEAD_SETUP_TIMEOUT`: its stdout, or what
/// went wrong in words.
pub fn run_program(platform: &dyn Platform, cmd: &Command) -> Result<String, String> {
    let out = platform
        .run(cmd, LEAD_SETUP_TIMEOUT)
        .map_err(|e| e.to_string())?;
    if out.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    let said = if stderr.is_empty() {
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    } else {
        stderr
    };
    Err(if out.timed_out {
        format!("timed out after {}s", LEAD_SETUP_TIMEOUT.as_secs())
    } else {
        format!("exit {}: {said}", out.exit_code)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_names_home_only_when_given_and_without_a_trailing_slash() {
        let mut c = LeadCommand {
            bin: PathBuf::from("/b/teamwire"),
            home: None,
        };
        assert_eq!(c.words(), ["/b/teamwire"]);
        c.home = Some(PathBuf::from("/h/x/"));
        assert_eq!(c.words(), ["/b/teamwire", "--home", "/h/x"]);
    }

    #[test]
    fn apply_stops_at_the_first_failure_and_names_what_was_done() {
        let e = apply_in_order(&["a", "b", "c"], |c| {
            if *c == "b" {
                Err("boom".to_owned())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(e.done, ["a"]);
        assert_eq!(e.failed, "b");
        assert_eq!(e.to_string(), "b failed: boom");
    }

    #[test]
    fn an_optional_setup_that_is_missing_passes_and_names_the_install() {
        let missing = optional_check(Ok(LeadStatus::Missing), "pi", "pi");
        assert_eq!(
            missing,
            CheckOutcome::Pass(
                "not installed; to lead from pi, run `teamwire install --harness pi`".to_owned()
            )
        );
        assert!(matches!(
            optional_check(Ok(LeadStatus::Stale(vec!["old".into()])), "pi", "pi"),
            CheckOutcome::Warn(t) if t.starts_with("old; ")
        ));
    }
}
