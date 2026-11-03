//! `lead_setup_contract`: what every `LeadSetup` must do (ADR 0031), on a
//! fake platform that never starts the harness's program.

use std::path::Path;

use teamwire_core::{
    LeadChange, LeadCommand, LeadSetup, LeadStatus, LeadStep, LeadTarget, PlatformError,
    PlatformErrorCode, RunOutput,
};

use super::{fail, ok, require};
use crate::FakePlatform;

/// How a caller lets `lead_setup_contract` reach one lead setup. The caller
/// builds it on `platform`, with the harness's own files in a temporary
/// directory where nothing is installed.
pub struct LeadSetupSetup<'a> {
    pub setup: &'a dyn LeadSetup,
    /// The platform `setup` was built on.
    pub platform: &'a FakePlatform,
    /// An empty state directory.
    pub home: &'a Path,
    pub command: LeadCommand,
    pub shared_skill: &'a str,
    /// Called before each `status` and plan: queues on `platform` what the
    /// harness's program answers when asked what is installed, for a harness
    /// that asks it (Codex's `plugin list`). Nothing is installed.
    pub before_read: &'a dyn Fn(&FakePlatform),
}

impl std::fmt::Debug for LeadSetupSetup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeadSetupSetup")
            .field("program", &self.setup.program())
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

/// Runs every rule on one lead setup.
pub fn lead_setup_contract(setup: &LeadSetupSetup<'_>) {
    let s = setup.setup;
    require("the program is named", !s.program().is_empty());
    require("the label is not empty", !s.label().is_empty());
    let target = LeadTarget {
        command: &setup.command,
        home: setup.home,
        shared_skill: setup.shared_skill,
        compare_files: true,
    };

    (setup.before_read)(setup.platform);
    match ok("status", s.status(&target)) {
        LeadStatus::Missing => {}
        other @ (LeadStatus::Stale(_) | LeadStatus::Current) => {
            fail("with nothing installed the status is missing", other)
        }
    }

    let written = setup.platform.written().len();
    let plan = plan_install(setup, &target);
    require("a fresh install plans changes", !plan.is_empty());
    require(
        "planning writes nothing",
        setup.platform.written().len() == written && !has_entries(setup.home),
    );
    for change in &plan {
        steps_stay_in_bounds(setup, change);
    }
    require(
        "a fresh install writes its files",
        plan.iter()
            .any(|c| matches!(c.step(), LeadStep::WriteFiles { .. })),
    );

    write_failure_changes_nothing(setup, &plan);
    partial_apply_names_what_was_done(setup, &plan);

    require(
        "a second plan skips the files already written",
        !plan_install(setup, &target)
            .iter()
            .any(|c| matches!(c.step(), LeadStep::WriteFiles { .. })),
    );

    (setup.before_read)(setup.platform);
    let removal = ok("plan_uninstall", s.plan_uninstall(setup.home, false)).changes;
    require(
        "uninstall deletes the files install wrote",
        removal
            .iter()
            .any(|c| matches!(c.step(), LeadStep::RemoveDir(_))),
    );
    for change in &removal {
        steps_stay_in_bounds(setup, change);
    }
    let deletions: Vec<LeadChange> = removal
        .into_iter()
        .filter(|c| matches!(c.step(), LeadStep::RemoveDir(_)))
        .collect();
    ok("apply of the deletions", s.apply(&deletions));
    require("after uninstall no file is left", !has_entries(setup.home));
    (setup.before_read)(setup.platform);
    require(
        "a second uninstall plans nothing",
        ok("plan_uninstall", s.plan_uninstall(setup.home, false))
            .changes
            .is_empty(),
    );
    ok("apply of nothing", s.apply(&[]));

    (setup.before_read)(setup.platform);
    let checks = s.doctor_checks(Ok(&target), setup.home);
    require("doctor has a check of the setup", !checks.is_empty());
    let checks = s.doctor_checks(Err("no binary"), setup.home);
    require(
        "every doctor check is named",
        checks.iter().all(|(name, _)| !name.is_empty()),
    );
}

fn plan_install(setup: &LeadSetupSetup<'_>, target: &LeadTarget<'_>) -> Vec<LeadChange> {
    (setup.before_read)(setup.platform);
    ok("plan_install", setup.setup.plan_install(target, false)).changes
}

/// A change says what it does, writes only in the state directory, and
/// runs no program but the harness's own.
fn steps_stay_in_bounds(setup: &LeadSetupSetup<'_>, change: &LeadChange) {
    require("a change says what it does", !change.to_string().is_empty());
    match change.step() {
        LeadStep::WriteFiles { dir, .. } | LeadStep::RemoveDir(dir) => require(
            "a lead setup's files sit in the state directory",
            dir.starts_with(setup.home),
        ),
        LeadStep::Run(cmd) => require(
            "a change runs only the harness's own program",
            cmd.program == setup.setup.program(),
        ),
        LeadStep::Own(_) => {}
    }
}

/// A full disk stops apply at its first change, before anything is made.
fn write_failure_changes_nothing(setup: &LeadSetupSetup<'_>, plan: &[LeadChange]) {
    setup.platform.fail_writes(Some(PlatformError::new(
        PlatformErrorCode::NoSpace,
        "contract: disk full",
    )));
    let result = setup.setup.apply(plan);
    setup.platform.fail_writes(None);
    match result {
        Err(e) if e.done.is_empty() && plan.first() == Some(&e.failed) => {}
        other => fail(
            "a write failure on the first change stops apply with nothing done",
            other,
        ),
    }
    require("a failed write leaves no file", !has_entries(setup.home));
}

/// A program that fails partway stops apply there: every change before it
/// was made, in order, and the failure carries what the program said.
fn partial_apply_names_what_was_done(setup: &LeadSetupSetup<'_>, plan: &[LeadChange]) {
    let Some(first_run) = plan
        .iter()
        .position(|c| matches!(c.step(), LeadStep::Run(_)))
    else {
        fail("a fresh install registers through the program", plan);
    };
    setup.platform.queue_run(Ok(RunOutput {
        exit_code: 1,
        stdout: Vec::new(),
        stderr: b"contract: refused".to_vec(),
        timed_out: false,
    }));
    let Err(e) = setup.setup.apply(plan) else {
        fail("a failing program fails apply", plan);
    };
    require(
        "the changes before the failure are done, in order",
        plan.get(..first_run) == Some(e.done.as_slice()),
    );
    require(
        "the failed change is the program's",
        plan.get(first_run) == Some(&e.failed),
    );
    require(
        "the failure carries what the program said",
        e.detail.contains("contract: refused"),
    );
    for change in &e.done {
        if let LeadStep::WriteFiles { dir, files } = change.step() {
            require(
                "a done write is on disk",
                files.iter().all(|(rel, _)| dir.join(rel).is_file()),
            );
        }
    }
}

fn has_entries(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}
