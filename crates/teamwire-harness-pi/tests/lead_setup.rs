//! The pi lead package (ADR 0030): its files, whether pi lists it, and the
//! plan and order of `pi install` and `pi remove`.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "the test's helpers build fixtures from literals and index JSON they just read"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes pi's settings and the package files as pi and install would"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use teamwire_core::RunOutput;
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::lead_setup::{Change, LeadCommand, LeadStatus, package_dir};
use teamwire_harness_pi::{Pi, PiConfig};

const SHARED: &str = "---\nname: teamwire\ndescription: d\n---\n\n# Body\n";

struct Bed {
    tmp: tempfile::TempDir,
    platform: Arc<FakePlatform>,
}

impl Bed {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("agent")).unwrap();
        std::fs::create_dir_all(tmp.path().join("home")).unwrap();
        Self {
            tmp,
            platform: Arc::new(FakePlatform::new()),
        }
    }

    fn agent(&self) -> PathBuf {
        self.tmp.path().join("agent")
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    fn pi(&self) -> Pi {
        Pi::new(
            Arc::clone(&self.platform) as Arc<dyn teamwire_core::Platform>,
            PiConfig {
                agent_dir: self.agent(),
                pi_bin: "pi".into(),
            },
        )
    }

    /// pi's settings with these package sources.
    fn packages(&self, sources: &[Value]) {
        std::fs::write(
            self.agent().join("settings.json"),
            json!({ "packages": sources, "defaultModel": "m" }).to_string(),
        )
        .unwrap();
    }

    /// The `pi` argv of each run, without the program.
    fn pi_runs(&self) -> Vec<Vec<String>> {
        self.platform
            .runs()
            .into_iter()
            .map(|(cmd, _)| cmd.args)
            .collect()
    }

    fn write_files(&self) {
        let changes = self
            .pi()
            .plan_install(&want(), SHARED, &self.home())
            .unwrap();
        let writes: Vec<_> = changes
            .into_iter()
            .filter(|c| matches!(c, Change::WritePackage { .. }))
            .collect();
        self.pi().apply_lead_setup(&writes).unwrap();
    }
}

fn want() -> LeadCommand {
    LeadCommand {
        bin: PathBuf::from("/opt/bin/teamwire"),
        home: Some(PathBuf::from("/scratch tw/")),
    }
}

fn file<'a>(files: &'a [(PathBuf, String)], path: &str) -> &'a str {
    files
        .iter()
        .find(|(p, _)| p == Path::new(path))
        .map(|(_, text)| text.as_str())
        .unwrap()
}

#[test]
fn the_package_holds_the_extension_the_skills_and_a_manifest() {
    let bed = Bed::new();
    let files = bed.pi().package_files(&want(), SHARED).unwrap();
    let paths: Vec<_> = files.iter().map(|(p, _)| p.display().to_string()).collect();
    assert_eq!(
        paths,
        [
            "package.json",
            "extensions/teamwire-lead.ts",
            "skills/teamwire/SKILL.md",
            "skills/teamwire/reference.md",
            "skills/promptwire/SKILL.md",
        ]
    );
    let manifest: Value = serde_json::from_str(file(&files, "package.json")).unwrap();
    assert_eq!(
        manifest["pi"]["extensions"],
        json!(["./extensions/teamwire-lead.ts"])
    );
    let extension = file(&files, "extensions/teamwire-lead.ts");
    assert!(
        extension.contains(r#""bin":"/opt/bin/teamwire""#),
        "{extension}"
    );
    assert!(
        extension.contains(r#""homeArgs":["--home","/scratch tw"]"#),
        "{extension}"
    );
    let state_dir = bed.agent().join("sessions/teamwire");
    assert!(extension.contains(&format!(r#""stateDir":"{}""#, state_dir.display())));
    let skill = file(&files, "skills/teamwire/SKILL.md");
    assert!(
        skill.starts_with("---\nname: teamwire\ndescription: d\n---\n\n## You are leading from pi")
    );
    assert!(skill.find("teamwire_watch").unwrap() < skill.find("# Body").unwrap());
}

#[test]
fn a_fresh_install_writes_the_files_then_adds_the_package() {
    let bed = Bed::new();
    let dir = package_dir(&bed.home());
    let changes = bed.pi().plan_install(&want(), SHARED, &bed.home()).unwrap();
    assert!(matches!(&changes[0], Change::WritePackage { dir: d, .. } if *d == dir));
    assert_eq!(changes[1], Change::AddPackage(dir.clone()));
    assert_eq!(changes.len(), 2);
    // The plan ran `pi --version` and nothing else.
    assert_eq!(bed.pi_runs(), [vec!["--version".to_owned()]]);

    bed.pi().apply_lead_setup(&changes).unwrap();
    assert!(dir.join("extensions/teamwire-lead.ts").is_file());
    let runs = bed.platform.runs();
    let (install, _) = runs.last().unwrap();
    assert_eq!(install.args, ["install", &dir.display().to_string()]);
    let env: Vec<_> = install
        .env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert!(env.contains(&("PI_CODING_AGENT_DIR", &*bed.agent().display().to_string())));
    assert!(env.contains(&("PI_OFFLINE", "1")));
}

#[test]
fn a_listed_package_with_current_files_needs_no_change_and_no_pi() {
    let bed = Bed::new();
    bed.write_files();
    // pi keeps a local path relative to its settings folder.
    bed.packages(&[json!("../home/pi-package"), json!("npm:other")]);
    let before = bed.pi_runs().len();
    let changes = bed.pi().plan_install(&want(), SHARED, &bed.home()).unwrap();
    assert_eq!(changes, []);
    assert_eq!(bed.pi_runs().len(), before, "no `pi` run");
    assert_eq!(
        bed.pi()
            .package_status(&want(), SHARED, &bed.home(), true)
            .unwrap(),
        LeadStatus::Current
    );
}

#[test]
fn the_status_is_missing_stale_or_current() {
    let bed = Bed::new();
    let status = |compare| {
        bed.pi()
            .package_status(&want(), SHARED, &bed.home(), compare)
            .unwrap()
    };
    assert_eq!(status(true), LeadStatus::Missing);
    // Listed in object form, files not written yet.
    let dir = package_dir(&bed.home());
    bed.packages(&[json!({ "source": dir.display().to_string() })]);
    assert!(matches!(status(true), LeadStatus::Stale(p) if p[0].contains("not this binary's")));
    assert!(matches!(status(false), LeadStatus::Stale(p) if p[0].contains("missing")));
    bed.write_files();
    assert_eq!(status(true), LeadStatus::Current);
    // Another binary's files: only a build skips the comparison.
    let other = LeadCommand {
        bin: PathBuf::from("/new/teamwire"),
        home: None,
    };
    let other_status = bed
        .pi()
        .package_status(&other, SHARED, &bed.home(), true)
        .unwrap();
    assert!(matches!(other_status, LeadStatus::Stale(_)));
    assert_eq!(
        bed.pi()
            .package_status(&other, SHARED, &bed.home(), false)
            .unwrap(),
        LeadStatus::Current
    );
}

#[test]
fn changed_files_are_written_again_without_a_second_pi_install() {
    let bed = Bed::new();
    bed.write_files();
    bed.packages(&[json!(package_dir(&bed.home()).display().to_string())]);
    let other = LeadCommand {
        bin: PathBuf::from("/new/teamwire"),
        home: None,
    };
    let changes = bed.pi().plan_install(&other, SHARED, &bed.home()).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], Change::WritePackage { .. }));
}

#[test]
fn a_pi_that_cannot_run_is_refused_before_anything_is_written() {
    let bed = Bed::new();
    bed.platform.queue_run(Ok(RunOutput {
        exit_code: 127,
        stdout: Vec::new(),
        stderr: b"pi: command not found".to_vec(),
        timed_out: false,
    }));
    let e = bed
        .pi()
        .plan_install(&want(), SHARED, &bed.home())
        .unwrap_err();
    assert!(e.to_string().contains("cannot run `pi`"), "{e}");
    assert!(!package_dir(&bed.home()).exists());
}

#[test]
fn unreadable_settings_are_a_refusal_that_names_the_file() {
    let bed = Bed::new();
    std::fs::write(bed.agent().join("settings.json"), "{ not json").unwrap();
    let e = bed
        .pi()
        .plan_install(&want(), SHARED, &bed.home())
        .unwrap_err();
    assert!(e.to_string().contains("settings.json"), "{e}");
}

#[test]
fn uninstall_removes_the_package_then_its_files() {
    let bed = Bed::new();
    bed.write_files();
    let dir = package_dir(&bed.home());
    bed.packages(&[json!(dir.display().to_string())]);
    let changes = bed.pi().plan_uninstall(&bed.home()).unwrap();
    assert_eq!(
        changes,
        [
            Change::RemovePackage(dir.clone()),
            Change::RemovePackageFiles(dir.clone())
        ]
    );
    bed.pi().apply_lead_setup(&changes).unwrap();
    assert!(!dir.exists());
    let runs = bed.pi_runs();
    assert_eq!(
        runs.last().unwrap(),
        &["remove", &dir.display().to_string()]
    );
}

#[test]
fn uninstall_with_nothing_installed_plans_nothing() {
    let bed = Bed::new();
    assert_eq!(bed.pi().plan_uninstall(&bed.home()).unwrap(), []);
}

#[test]
fn apply_stops_at_the_first_failure_and_names_what_was_done() {
    let bed = Bed::new();
    let dir = package_dir(&bed.home());
    bed.platform.queue_run(Ok(RunOutput {
        exit_code: 1,
        stdout: Vec::new(),
        stderr: b"no such package".to_vec(),
        timed_out: false,
    }));
    let changes = [
        Change::AddPackage(dir.clone()),
        Change::RemovePackageFiles(dir.clone()),
    ];
    let e = bed.pi().apply_lead_setup(&changes).unwrap_err();
    assert_eq!(e.failed, Change::AddPackage(dir));
    assert!(e.done.is_empty());
    assert!(e.to_string().contains("no such package"), "{e}");
}
