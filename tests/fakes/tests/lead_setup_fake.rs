//! `FakeLeadSetup` passes the lead-setup contract suite, the suite fails for
//! a setup that breaks one of its rules, and `FakeHarness` hands the fake
//! out only when a test gives it one.

use std::path::Path;
use std::sync::Arc;

use teamwire_core::lead_setup::DoctorCheck;
use teamwire_core::{
    ApplyError, Harness, LeadChange, LeadCommand, LeadPlan, LeadSetup, LeadSetupError, LeadStatus,
    LeadTarget,
};
use teamwire_fakes::contract::{LeadSetupSetup, lead_setup_contract};
use teamwire_fakes::{FakeHarness, FakeLeadSetup, FakePlatform, TestHome};

fn command() -> LeadCommand {
    LeadCommand {
        bin: "/opt/bin/teamwire".into(),
        home: None,
    }
}

fn run(setup: &dyn LeadSetup, platform: &FakePlatform, home: &Path) {
    lead_setup_contract(&LeadSetupSetup {
        setup,
        platform,
        home,
        command: command(),
        shared_skill: "---\nname: teamwire\n---\n",
    });
}

#[test]
fn the_fake_lead_setup_passes_the_lead_setup_contract() {
    let home = TestHome::new().unwrap();
    let platform = Arc::new(FakePlatform::new());
    let fake = FakeLeadSetup::new(platform.clone());
    run(&fake, &platform, home.path());
}

#[test]
fn the_fake_installs_reports_current_and_uninstalls() {
    let home = TestHome::new().unwrap();
    let platform = Arc::new(FakePlatform::new());
    let fake = FakeLeadSetup::new(platform.clone());
    let command = command();
    let target = LeadTarget {
        command: &command,
        home: home.path(),
        shared_skill: "skill",
        compare_files: true,
    };
    let plan = fake.plan_install(&target, false).unwrap();
    assert_eq!(plan.changes.len(), 2);
    assert!(plan.note.is_some());
    fake.apply(&plan.changes).unwrap();
    assert!(fake.is_registered());
    assert_eq!(fake.status(&target).unwrap(), LeadStatus::Current);
    assert!(
        fake.plan_install(&target, false)
            .unwrap()
            .changes
            .is_empty()
    );
    let removal = fake.plan_uninstall(home.path(), false).unwrap();
    fake.apply(&removal.changes).unwrap();
    assert!(!fake.is_registered());
    assert_eq!(fake.status(&target).unwrap(), LeadStatus::Missing);
    assert!(!home.path().join(FakeLeadSetup::DIR).exists());
}

#[test]
fn a_fake_harness_leads_only_when_given_a_lead_setup() {
    assert!(FakeHarness::new().lead_setup().is_none());
    let lead = FakeLeadSetup::new(Arc::new(FakePlatform::new())).as_default_lead();
    let harness = FakeHarness::new().with_lead_setup(lead);
    let setup = harness.lead_setup().unwrap();
    assert!(setup.default_lead());
    assert_eq!(setup.program(), "fake-agent");
}

/// A setup that names one program and runs another.
struct OtherProgram(FakeLeadSetup);

impl LeadSetup for OtherProgram {
    fn program(&self) -> &'static str {
        "claimed-agent"
    }
    fn label(&self) -> &'static str {
        self.0.label()
    }
    fn status(&self, target: &LeadTarget<'_>) -> Result<LeadStatus, LeadSetupError> {
        self.0.status(target)
    }
    fn plan_install(
        &self,
        target: &LeadTarget<'_>,
        force: bool,
    ) -> Result<LeadPlan, LeadSetupError> {
        self.0.plan_install(target, force)
    }
    fn plan_uninstall(&self, home: &Path, force: bool) -> Result<LeadPlan, LeadSetupError> {
        self.0.plan_uninstall(home, force)
    }
    fn apply(&self, changes: &[LeadChange]) -> Result<(), Box<ApplyError>> {
        self.0.apply(changes)
    }
    fn doctor_checks(&self, target: Result<&LeadTarget<'_>, &str>, dir: &Path) -> Vec<DoctorCheck> {
        self.0.doctor_checks(target, dir)
    }
}

#[test]
#[should_panic(expected = "contract violated: a change runs only the harness's own program")]
fn the_contract_fails_for_a_setup_that_runs_another_program() {
    let home = TestHome::new().unwrap();
    let platform = Arc::new(FakePlatform::new());
    let liar = OtherProgram(FakeLeadSetup::new(platform.clone()));
    run(&liar, &platform, home.path());
}
