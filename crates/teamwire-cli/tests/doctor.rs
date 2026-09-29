//! `teamwire doctor`: the checks run in-process against a fake platform (a
//! network file system, a dead process and a full disk cannot be made real),
//! and the binary runs once on a healthy setup and once on a broken one.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and git"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    dead_code,
    reason = "the included command file's `run` is only called by the binary"
)]

use std::path::{Path, PathBuf};
use teamwire_core::{
    Clock, HarnessId, HostId, LeadRoute, Pid, PlatformError, PlatformErrorCode, RunOutput, TeamName,
};
use teamwire_fakes::{FakeBins, FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_store::{NewTeam, Store};

/// The command file is compiled here with the two crate-level items it names
/// (`Global`, `wiring::Wiring`), so the checks run against a fake platform.
#[derive(Debug)]
pub(crate) struct Global {
    pub(crate) home: Option<PathBuf>,
}

mod wiring {
    use std::sync::Arc;

    use teamwire_core::Platform;

    pub(crate) struct Wiring {
        pub(crate) platform: Arc<dyn Platform>,
    }
}

#[path = "../src/cmd/doctor.rs"]
mod doctor;

use doctor::{Check, Host, Inputs, Level, diagnose, exit_code};

struct Bed {
    home: TestHome,
    claude: TestHome,
    bin: TestHome,
    platform: FakePlatform,
    host: Host,
}

impl Bed {
    /// A setup on which every check passes.
    fn healthy() -> Self {
        let bed = Self {
            home: TestHome::new().unwrap(),
            claude: TestHome::new().unwrap(),
            bin: TestHome::new().unwrap(),
            platform: FakePlatform::new(),
            host: Host::Herdr,
        };
        std::fs::write(
            bed.claude.path().join("settings.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[{"command":"bash herdr-agent-state.sh session"}]}]}}"#,
        )
        .unwrap();
        for program in ["claude", "herdr"] {
            std::fs::write(bed.bin.path().join(program), "#!/bin/sh\n").unwrap();
        }
        bed
    }

    fn checks(&self) -> Vec<Check> {
        diagnose(&Inputs {
            host: self.host,
            home: self.home.path().to_path_buf(),
            platform: &self.platform,
            path_var: Some(self.bin.path().as_os_str().to_owned()),
            claude_dir: self.claude.path().to_path_buf(),
            repo: self.home.path().to_path_buf(),
        })
    }

    fn team(&self, name: &str) -> Store {
        let team = TeamName::parse(name).unwrap();
        let mut store = Store::open(self.home.path(), &team).unwrap();
        let now = FakeClock::new().now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: team.clone(),
                        title: "T".to_owned(),
                        lead_harness: HarnessId::from_static(FakeHarness::ID),
                        lead_host: HostId::from_static(FakeHost::ID),
                        lead_route: LeadRoute::Sink,
                        lead_session: None,
                    },
                )
                .map(|_| ())
            })
            .unwrap();
        store
    }
}

fn level_of(checks: &[Check], name: &str) -> Level {
    checks
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.level)
        .unwrap_or_else(|| unreachable!("no check named {name}: {checks:?}"))
}

fn failing(checks: &[Check]) -> Vec<&str> {
    checks
        .iter()
        .filter(|c| c.level != Level::Pass)
        .map(|c| c.name.as_str())
        .collect()
}

#[test]
fn a_healthy_setup_passes_every_check_and_exits_zero() {
    let bed = Bed::healthy();
    drop(bed.team("alpha"));
    let checks = bed.checks();
    assert_eq!(failing(&checks), Vec::<&str>::new(), "{checks:?}");
    assert_eq!(level_of(&checks, "team alpha quick_check"), Level::Pass);
    assert_eq!(exit_code(&checks), 0);
}

#[test]
fn network_fs_warned() {
    let bed = Bed::healthy();
    bed.platform.set_network_fs(true);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "network filesystem"), Level::Warn);
    let text = &checks
        .iter()
        .find(|c| c.name == "network filesystem")
        .unwrap()
        .detail;
    assert!(text.contains("network filesystem"), "{text}");
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn doctor_reports_quick_check_failure() {
    let bed = Bed::healthy();
    let store = bed.team("alpha");
    drop(store);
    // Overwrite two pages in the middle of the file; the header stays valid,
    // so the database still opens and only `quick_check` sees the damage.
    let path = teamwire_store::database_path(bed.home.path(), &TeamName::parse("alpha").unwrap());
    let mut bytes = std::fs::read(&path).unwrap();
    assert!(
        bytes.len() > 4 * 4096,
        "the database is too small to damage"
    );
    bytes[2 * 4096..4 * 4096].fill(0xAB);
    std::fs::write(&path, bytes).unwrap();

    let checks = bed.checks();
    assert_eq!(
        level_of(&checks, "team alpha quick_check"),
        Level::Fail,
        "{checks:?}"
    );
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn a_database_of_another_schema_version_fails() {
    let bed = Bed::healthy();
    drop(bed.team("alpha"));
    let path = teamwire_store::database_path(bed.home.path(), &TeamName::parse("alpha").unwrap());
    // `PRAGMA user_version` is bytes 60..64 of the database header.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[60..64].copy_from_slice(&9_u32.to_be_bytes());
    std::fs::write(&path, bytes).unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha database"), Level::Fail);
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn a_close_lock_held_by_a_dead_process_is_reported_with_the_fix() {
    let bed = Bed::healthy();
    let mut store = bed.team("alpha");
    let now = FakeClock::new().now();
    store
        .write(|tx| tx.begin_close(now, Pid::new(4242)).map(|_| ()))
        .unwrap();
    drop(store);

    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha close lock"), Level::Fail);
    let detail = &checks
        .iter()
        .find(|c| c.name == "team alpha close lock")
        .unwrap()
        .detail;
    assert!(detail.contains("--clear-stale-lock"), "{detail}");

    bed.platform.set_alive(Pid::new(4242), true);
    let checks = bed.checks();
    assert!(
        checks.iter().all(|c| c.name != "team alpha close lock"),
        "a live owner is not stale: {checks:?}"
    );
}

#[test]
fn a_full_disk_fails() {
    let bed = Bed::healthy();
    bed.platform.set_free_bytes(10 * 1024 * 1024);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "free disk"), Level::Fail);
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn a_platform_that_cannot_answer_is_a_warning_not_a_pass() {
    let bed = Bed::healthy();
    bed.platform
        .fail_free_bytes(Some(PlatformError::new(PlatformErrorCode::Io, "boom")));
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "free disk"), Level::Warn);
}

#[test]
fn a_repo_that_does_not_ignore_worktrees_fails() {
    let bed = Bed::healthy();
    bed.platform.queue_run(Ok(RunOutput {
        exit_code: 1,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: false,
    }));
    let checks = bed.checks();
    assert_eq!(level_of(&checks, ".worktrees ignored"), Level::Fail);
    let runs = bed.platform.runs();
    assert_eq!(runs[0].0.program, "git");
    assert_eq!(runs[0].0.args, ["check-ignore", "-q", ".worktrees/probe"]);
}

#[test]
fn outside_a_repository_the_ignore_check_passes() {
    let bed = Bed::healthy();
    bed.platform.queue_run(Ok(RunOutput {
        exit_code: 128,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: false,
    }));
    assert_eq!(level_of(&bed.checks(), ".worktrees ignored"), Level::Pass);
}

#[test]
fn a_missing_harness_or_host_binary_fails_and_the_other_host_is_not_asked_about() {
    let bed = Bed::healthy();
    std::fs::remove_file(bed.bin.path().join("herdr")).unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "herdr on PATH"), Level::Fail);
    assert_eq!(level_of(&checks, "claude on PATH"), Level::Pass);
    assert!(!names(&checks).contains(&"tmux on PATH"));
}

#[test]
fn a_settings_file_without_the_herdr_hook_fails() {
    let bed = Bed::healthy();
    std::fs::write(bed.claude.path().join("settings.json"), "{}").unwrap();
    assert_eq!(level_of(&bed.checks(), "herdr session hook"), Level::Fail);
}

#[test]
fn doctor_changes_nothing() {
    let bed = Bed::healthy();
    drop(bed.team("alpha"));
    let before = std::fs::read(teamwire_store::database_path(
        bed.home.path(),
        &TeamName::parse("alpha").unwrap(),
    ))
    .unwrap();
    bed.checks();
    let after = std::fs::read(teamwire_store::database_path(
        bed.home.path(),
        &TeamName::parse("alpha").unwrap(),
    ))
    .unwrap();
    assert_eq!(before, after);
    assert!(bed.platform.written().is_empty());
}

struct Run {
    code: i32,
    out: String,
}

fn teamwire(bed: &Bed, path: &str, cwd: &Path) -> Run {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .arg("doctor")
        .current_dir(cwd)
        .env("HERDR_ENV", "1")
        .env_remove("TMUX")
        .env("PATH", path)
        .env("TEAMWIRE_CLAUDE_DIR", bed.claude.path())
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
    }
}

fn ignoring_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::write(dir.path().join(".gitignore"), ".worktrees/\n").unwrap();
    dir
}

#[test]
fn the_binary_exits_zero_when_all_pass_and_two_when_one_fails() {
    let bed = Bed::healthy();
    drop(bed.team("alpha"));
    let bins = FakeBins::ensure().unwrap();
    let fake_dir = bed.bin.path().join("fakes");
    bins.install(&fake_dir).unwrap();
    let repo = ignoring_repo();
    let path = format!("{}:/usr/bin:/bin", fake_dir.display());

    let ok = teamwire(&bed, &path, repo.path());
    assert_eq!(ok.code, 0, "{}", ok.out);
    assert!(
        ok.out.contains("ok   team alpha quick_check: ok"),
        "{}",
        ok.out
    );

    let no_binaries = teamwire(&bed, "/usr/bin:/bin", repo.path());
    assert_eq!(no_binaries.code, 2, "{}", no_binaries.out);
    assert!(
        no_binaries.out.contains("FAIL claude on PATH"),
        "{}",
        no_binaries.out
    );
}

fn names(checks: &[Check]) -> Vec<&str> {
    checks.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn the_host_in_use_comes_from_the_environment() {
    assert_eq!(Host::detect(Some("1"), Some("/tmp/tmux")), Host::Herdr);
    assert_eq!(
        Host::detect(None, Some("/tmp/tmux-1/default,1,0")),
        Host::Tmux
    );
    assert_eq!(Host::detect(Some("0"), Some("/tmp/tmux")), Host::Tmux);
    assert_eq!(Host::detect(None, Some("")), Host::None);
    assert_eq!(Host::detect(None, None), Host::None);
}

#[test]
fn a_tmux_only_setup_passes_without_herdr_or_its_hook() {
    let mut bed = Bed::healthy();
    bed.host = Host::Tmux;
    std::fs::remove_file(bed.bin.path().join("herdr")).unwrap();
    std::fs::write(bed.bin.path().join("tmux"), "#!/bin/sh\n").unwrap();
    std::fs::write(bed.claude.path().join("settings.json"), "{}").unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), Vec::<&str>::new(), "{checks:?}");
    assert!(!names(&checks).contains(&"herdr session hook"));
    assert!(!names(&checks).contains(&"herdr on PATH"));
    assert_eq!(exit_code(&checks), 0);
}

#[test]
fn a_tmux_setup_without_tmux_on_path_fails() {
    let mut bed = Bed::healthy();
    bed.host = Host::Tmux;
    assert_eq!(level_of(&bed.checks(), "tmux on PATH"), Level::Fail);
}

#[test]
fn with_no_host_doctor_says_so_and_requires_neither_host_tool() {
    let mut bed = Bed::healthy();
    bed.host = Host::None;
    std::fs::remove_file(bed.bin.path().join("herdr")).unwrap();
    std::fs::write(bed.claude.path().join("settings.json"), "{}").unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), Vec::<&str>::new(), "{checks:?}");
    assert_eq!(level_of(&checks, "host"), Level::Pass);
    assert!(!names(&checks).contains(&"herdr session hook"));
}
