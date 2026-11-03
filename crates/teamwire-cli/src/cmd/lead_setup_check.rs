//! The harness checks of `teamwire doctor`: one loop over the registered
//! harnesses (ADR 0031) for each one's program on PATH, its version, its own
//! program checks, and its lead setup. Reads only. Kept apart from
//! `doctor.rs` and free of its types, so the doctor tests compile this file
//! as it is.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use teamwire_core::{CheckOutcome, LeadTarget, Registry};

/// Runs a program, by its path, with arguments: its output, or why it gave
/// none.
pub(crate) type PathProbe<'a> = &'a dyn Fn(&Path, &[&str]) -> Result<String, String>;

/// What the checks look at.
pub(crate) struct HarnessInputs<'a> {
    pub(crate) registry: &'a Registry,
    /// `$PATH`, where each harness's program is looked for.
    pub(crate) path_var: Option<&'a OsString>,
    pub(crate) probe: PathProbe<'a>,
    /// What each lead setup is checked against, or why it cannot be worked
    /// out.
    pub(crate) target: Result<LeadTarget<'a>, String>,
    /// The folder doctor runs in.
    pub(crate) dir: &'a Path,
}

/// True when the binary sits under a `target/` directory: a cargo build,
/// not an installed binary.
pub(crate) fn is_build_output(bin: &Path) -> bool {
    bin.components().any(|c| c.as_os_str() == "target")
}

pub(crate) fn on_path(path_var: Option<&OsString>, program: &str) -> Option<PathBuf> {
    let dirs = std::env::split_paths(path_var?);
    dirs.map(|d| d.join(program)).find(|p| p.is_file())
}

/// `<program> version`: the first line of the answer to `flag`, which is the
/// version itself.
pub(crate) fn version_check(
    probe: PathProbe<'_>,
    program: &str,
    path: &Path,
    flag: &str,
) -> (String, CheckOutcome) {
    let outcome = match probe(path, &[flag]) {
        Ok(text) => CheckOutcome::Pass(text.lines().next().unwrap_or("").trim().to_owned()),
        Err(e) => CheckOutcome::Warn(format!("`{program} {flag}` failed: {e}")),
    };
    (format!("{program} version"), outcome)
}

/// The checks of each harness's program: on PATH, its version, and its own
/// checks. `default_lead` picks the default lead harness, whose missing
/// program fails, or every other one, which doctor names only when found.
pub(crate) fn program_checks(
    inputs: &HarnessInputs<'_>,
    default_lead: bool,
) -> Vec<(String, CheckOutcome)> {
    let mut checks = Vec::new();
    for (_, setup) in inputs.registry.lead_setups() {
        if setup.default_lead() != default_lead {
            continue;
        }
        let program = setup.program();
        let name = format!("{program} on PATH");
        let Some(path) = on_path(inputs.path_var, program) else {
            if default_lead {
                checks.push((
                    name,
                    CheckOutcome::Fail(format!("the harness binary `{program}` is not on PATH")),
                ));
            }
            continue;
        };
        checks.push((
            name,
            CheckOutcome::Pass(format!("harness binary at {}", path.display())),
        ));
        checks.push(version_check(inputs.probe, program, &path, "--version"));
        let probe = |args: &[&str]| (inputs.probe)(&path, args);
        checks.extend(
            setup
                .program_checks(&probe)
                .into_iter()
                .map(|(n, o)| (n.to_owned(), o)),
        );
    }
    checks
}

/// Each lead setup's checks: the default lead harness's always, another's
/// when its program is on PATH.
pub(crate) fn lead_checks(inputs: &HarnessInputs<'_>) -> Vec<(String, CheckOutcome)> {
    let target = inputs.target.as_ref().map_err(String::as_str);
    inputs
        .registry
        .lead_setups()
        .into_iter()
        .filter(|(_, s)| s.default_lead() || on_path(inputs.path_var, s.program()).is_some())
        .flat_map(|(_, s)| s.doctor_checks(target, inputs.dir))
        .map(|(n, o)| (n.to_owned(), o))
        .collect()
}
