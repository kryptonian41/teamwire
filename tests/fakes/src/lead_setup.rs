//! `FakeLeadSetup`: an in-process `LeadSetup` (ADR 0031) whose files go
//! through the platform it is given, and whose "program" only flips a flag,
//! so install, uninstall and doctor run without a real harness.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use teamwire_core::lead_setup::{DoctorCheck, apply_in_order, make_step, optional_check};
use teamwire_core::{
    ApplyError, Command, LeadChange, LeadPlan, LeadSetup, LeadSetupError, LeadStatus, LeadStep,
    LeadTarget, Platform,
};

use crate::calls::lock;

/// The lead setup of a harness that does not exist. Its files sit in
/// `<home>/fake-lead/`; `<program> add|remove <dir>` registers them, which
/// the fake records instead of running.
pub struct FakeLeadSetup {
    platform: Arc<dyn Platform>,
    program: String,
    default_lead: bool,
    registered: Mutex<bool>,
}

impl std::fmt::Debug for FakeLeadSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeLeadSetup")
            .field("program", &self.program)
            .field("default_lead", &self.default_lead)
            .field("registered", &self.is_registered())
            .finish_non_exhaustive()
    }
}

impl FakeLeadSetup {
    /// The folder under the state directory that holds the fake's files.
    pub const DIR: &'static str = "fake-lead";

    /// An optional harness whose program is `fake-agent`, with nothing
    /// installed.
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            program: "fake-agent".to_owned(),
            default_lead: false,
            registered: Mutex::new(false),
        }
    }

    /// The program it reports and runs its steps with.
    #[must_use]
    pub fn with_program(mut self, program: &str) -> Self {
        program.clone_into(&mut self.program);
        self
    }

    #[must_use]
    pub fn as_default_lead(mut self) -> Self {
        self.default_lead = true;
        self
    }

    /// Whether `<program> add` ran last, not `remove`.
    pub fn is_registered(&self) -> bool {
        *lock(&self.registered)
    }

    fn files(target: &LeadTarget<'_>) -> Vec<(PathBuf, String)> {
        vec![
            (
                PathBuf::from("command.txt"),
                target.command.words().join(" "),
            ),
            (PathBuf::from("SKILL.md"), target.shared_skill.to_owned()),
        ]
    }

    fn written(dir: &Path, files: &[(PathBuf, String)]) -> bool {
        files
            .iter()
            .all(|(rel, text)| std::fs::read_to_string(dir.join(rel)).is_ok_and(|t| &t == text))
    }

    fn run(&self, verb: &str, dir: &Path) -> LeadChange {
        LeadChange::new(
            format!("{verb} the fake lead setup at {}", dir.display()),
            LeadStep::Run(
                Command::new(self.program.clone())
                    .arg(verb)
                    .arg(dir.display().to_string()),
            ),
        )
    }

    fn plan(changes: Vec<LeadChange>) -> LeadPlan {
        let note = (!changes.is_empty()).then_some("the fake harness sees the change at start");
        LeadPlan { changes, note }
    }
}

impl LeadSetup for FakeLeadSetup {
    fn program(&self) -> &str {
        &self.program
    }

    fn label(&self) -> &'static str {
        "the fake harness"
    }

    fn default_lead(&self) -> bool {
        self.default_lead
    }

    fn status(&self, target: &LeadTarget<'_>) -> Result<LeadStatus, LeadSetupError> {
        let dir = target.home.join(Self::DIR);
        if !self.is_registered() {
            return Ok(LeadStatus::Missing);
        }
        if target.compare_files && !Self::written(&dir, &Self::files(target)) {
            return Ok(LeadStatus::Stale(vec![format!(
                "the files at {} are not this binary's",
                dir.display()
            )]));
        }
        Ok(LeadStatus::Current)
    }

    fn plan_install(
        &self,
        target: &LeadTarget<'_>,
        _force: bool,
    ) -> Result<LeadPlan, LeadSetupError> {
        let dir = target.home.join(Self::DIR);
        let files = Self::files(target);
        let mut changes = Vec::new();
        if !Self::written(&dir, &files) {
            changes.push(LeadChange::new(
                format!("write the fake lead setup to {}", dir.display()),
                LeadStep::WriteFiles {
                    dir: dir.clone(),
                    files,
                },
            ));
        }
        if !self.is_registered() {
            changes.push(self.run("add", &dir));
        }
        Ok(Self::plan(changes))
    }

    fn plan_uninstall(&self, home: &Path, _force: bool) -> Result<LeadPlan, LeadSetupError> {
        let dir = home.join(Self::DIR);
        let mut changes = Vec::new();
        if self.is_registered() {
            changes.push(self.run("remove", &dir));
        }
        if dir.exists() {
            changes.push(LeadChange::new(
                format!("delete the fake lead setup at {}", dir.display()),
                LeadStep::RemoveDir(dir),
            ));
        }
        Ok(Self::plan(changes))
    }

    fn apply(&self, changes: &[LeadChange]) -> Result<(), Box<ApplyError>> {
        apply_in_order(changes, |c| {
            make_step(self.platform.as_ref(), c.step())?;
            if let LeadStep::Run(cmd) = c.step() {
                match cmd.args.first().map(String::as_str) {
                    Some("add") => *lock(&self.registered) = true,
                    Some("remove") => *lock(&self.registered) = false,
                    _ => {}
                }
            }
            Ok(())
        })
    }

    fn doctor_checks(
        &self,
        target: Result<&LeadTarget<'_>, &str>,
        _dir: &Path,
    ) -> Vec<DoctorCheck> {
        let outcome = match target {
            Ok(target) => optional_check(self.status(target), "fake", self.label()),
            Err(e) => teamwire_core::CheckOutcome::Warn(e.to_owned()),
        };
        vec![("fake lead setup", outcome)]
    }
}
