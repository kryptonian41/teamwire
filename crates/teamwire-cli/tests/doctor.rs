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
use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    Clock, HarnessId, HostId, LeadRoute, Pid, PlatformError, PlatformErrorCode, RunOutput,
    TeamName, Timestamp,
};
use teamwire_fakes::{FakeBins, FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig, McpEntry};
use teamwire_store::{NewTeam, Store};

#[path = "support/schema_1.rs"]
mod schema_1;

/// The command file is compiled here with the crate-level items it names
/// (`Global`, `wiring::Wiring`, `cmd::send::home_or_refuse`,
/// `cmd::install::wanted_entry`) and the real `cmd::lead_setup_check`, so
/// the checks run against a fake platform.
#[derive(Debug)]
pub(crate) struct Global {
    pub(crate) home: Option<PathBuf>,
}

mod wiring {
    use std::sync::Arc;

    use teamwire_core::Platform;
    pub(crate) use teamwire_harness_claude::{
        ClaudeConfig, McpEntry, McpState, SkillState, edited_skill, mcp_state, shadowing_entries,
        skill_state,
    };

    pub(crate) struct Wiring {
        pub(crate) platform: Arc<dyn Platform>,
        pub(crate) claude: ClaudeConfig,
    }
}

#[path = "../src/cmd/lead_setup_check.rs"]
mod lead_setup_check;

mod cmd {
    pub(crate) use crate::lead_setup_check;

    pub(crate) mod install {
        use crate::wiring::McpEntry;

        use crate::Global;

        /// The tests pass the wanted entry in `Inputs`; the real helper is not compiled here.
        pub(crate) fn wanted_entry(_global: &Global) -> Result<McpEntry, String> {
            Err("not compiled in this test".to_owned())
        }
    }

    pub(crate) mod send {
        use std::path::PathBuf;
        use std::process::ExitCode;

        use crate::Global;

        /// The state directory the test names; the real helper is not compiled here.
        pub(crate) fn home_or_refuse(_command: &str, global: &Global) -> Result<PathBuf, ExitCode> {
            global.home.clone().ok_or(ExitCode::from(1))
        }
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
        bed.install_lead_setup();
        bed
    }

    fn claude_config(&self) -> ClaudeConfig {
        ClaudeConfig {
            claude_dir: self.claude.path().to_path_buf(),
            claude_json: self.claude.path().join(".claude.json"),
            claude_bin: "claude".to_owned(),
        }
    }

    /// The entry the teamwire binary under test would register for this
    /// bed's state directory.
    fn lead_entry(&self) -> McpEntry {
        let bin = std::fs::canonicalize(env!("CARGO_BIN_EXE_teamwire")).unwrap();
        McpEntry::lead(&bin, Some(self.home.path()))
    }

    /// Writes the plugin files through the harness, on a platform of its own
    /// so the bed's platform records no writes, and Claude's own registration
    /// of the plugin as `claude plugin install` leaves it: loaded from the
    /// marketplace directory.
    fn install_lead_setup(&self) {
        let setup = std::sync::Arc::new(FakePlatform::new());
        let claude = ClaudeCode::new(setup, self.claude_config());
        let changes = claude
            .plan_install(&self.lead_entry(), self.home.path(), false)
            .unwrap();
        claude.apply_lead_setup(&changes).unwrap();
        self.register_plugin(&self.plugin_root(), true);
    }

    fn plugin_root(&self) -> PathBuf {
        self.home.path().join("claude-plugin/teamwire")
    }

    /// `installed_plugins.json` and `enabledPlugins`, with the plugin loaded
    /// from `root`.
    fn register_plugin(&self, root: &Path, enabled: bool) {
        let plugins = self.claude.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let installed = serde_json::json!({"version": 2, "plugins": {"teamwire@teamwire": [
            {"scope": "user", "installPath": root}]}});
        std::fs::write(
            plugins.join("installed_plugins.json"),
            installed.to_string(),
        )
        .unwrap();
        let path = self.claude.path().join("settings.json");
        let mut settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_else(|_| serde_json::json!({}));
        settings["enabledPlugins"] = serde_json::json!({"teamwire@teamwire": enabled});
        std::fs::write(path, settings.to_string()).unwrap();
    }

    /// The skill and MCP entry that ADR 0012 wrote.
    fn put_old_setup(&self, skill: &str, entry: &str) {
        let path = self.claude.path().join("skills/teamwire/SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, skill).unwrap();
        std::fs::write(self.claude.path().join(".claude.json"), entry).unwrap();
    }

    fn checks(&self) -> Vec<Check> {
        self.checks_as(false)
    }

    /// The checks as a binary under `target/` (`build`) or an installed one.
    fn checks_as(&self, build: bool) -> Vec<Check> {
        diagnose(&Inputs {
            host: self.host,
            home: self.home.path().to_path_buf(),
            platform: &self.platform,
            path_var: Some(self.bin.path().as_os_str().to_owned()),
            claude: self.claude_config(),
            lead_entry: Ok(self.lead_entry()),
            running_a_build: build,
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

/// A team no read-write open has migrated since an upgrade is healthy: its
/// rows read, and the next write migrates it.
#[test]
fn a_database_at_an_older_schema_passes_and_says_it_will_be_migrated() {
    let bed = Bed::healthy();
    schema_1::version_1_team(bed.home.path(), "alpha", 1).unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), Vec::<&str>::new(), "{checks:?}");
    let detail = &checks
        .iter()
        .find(|c| c.name == "team alpha schema")
        .unwrap()
        .detail;
    assert!(detail.contains("version 1"), "{detail}");
    assert_eq!(exit_code(&checks), 0);
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

/// A close lock whose pid now belongs to a process that started later is the
/// lock of a dead close: doctor reports it, as `close --clear-stale-lock`
/// would clear it. The original owner, still running, is not reported.
#[test]
fn a_close_lock_whose_pid_was_reused_is_reported() {
    let bed = Bed::healthy();
    let mut store = bed.team("alpha");
    let now = FakeClock::new().now();
    let pid = Pid::new(4343);
    let started = Timestamp::from_unix_millis(now.unix_millis() - 60_000).unwrap();
    bed.platform.set_alive(pid, true);
    bed.platform.set_started(pid, Some(started));
    store
        .write(|tx| {
            tx.begin_close(
                now,
                ProcessRef {
                    pid,
                    started: Some(started),
                },
            )
            .map(|_| ())
        })
        .unwrap();
    drop(store);
    let checks = bed.checks();
    assert!(
        checks.iter().all(|c| c.name != "team alpha close lock"),
        "the owner still runs: {checks:?}"
    );

    bed.platform.reuse_pid(pid, now);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha close lock"), Level::Fail);
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

fn detail_of<'a>(checks: &'a [Check], name: &str) -> &'a str {
    checks
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.detail.as_str())
        .unwrap_or_else(|| unreachable!("no check named {name}: {checks:?}"))
}

#[test]
fn a_missing_plugin_fails_and_names_install_as_the_fix() {
    let bed = Bed::healthy();
    std::fs::remove_file(bed.claude.path().join("plugins/installed_plugins.json")).unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), ["teamwire plugin"]);
    assert!(detail_of(&checks, "teamwire plugin").contains("not installed"));
    assert!(detail_of(&checks, "teamwire plugin").contains("run `teamwire install`"));
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn a_plugin_for_another_binary_or_a_disabled_one_fails() {
    let bed = Bed::healthy();
    // Claude Code's copy, made by an older binary.
    let copy = bed.claude.path().join("plugins/cache/teamwire");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(
        copy.join(".mcp.json"),
        r#"{"mcpServers":{"teamwire":{"command":"/old/teamwire","args":["mcp","--lead"]}}}"#,
    )
    .unwrap();
    bed.register_plugin(&copy, true);
    let checks = bed.checks();
    assert_eq!(failing(&checks), ["teamwire plugin"]);
    assert!(
        detail_of(&checks, "teamwire plugin").contains("another version"),
        "{checks:?}"
    );

    bed.register_plugin(&bed.plugin_root(), false);
    let checks = bed.checks();
    assert!(
        detail_of(&checks, "teamwire plugin").contains("disabled"),
        "{checks:?}"
    );
    assert!(detail_of(&checks, "teamwire plugin").contains("run `teamwire install`"));
}

#[test]
fn the_skill_and_entry_from_the_old_install_fail_and_name_install_as_the_fix() {
    let bed = Bed::healthy();
    assert_eq!(level_of(&bed.checks(), "old lead setup"), Level::Pass);
    let entry = r#"{"mcpServers":{"teamwire":{"command":"teamwire","args":["mcp","--lead"]}}}"#;
    bed.put_old_setup(teamwire_harness_claude::SKILL_TEXT, entry);
    let checks = bed.checks();
    assert_eq!(failing(&checks), ["old lead setup"]);
    let detail = detail_of(&checks, "old lead setup");
    assert!(detail.contains("the old skill at"), "{detail}");
    assert!(detail.contains("the old user MCP server"), "{detail}");
    assert!(detail.ends_with("run `teamwire install`"), "{detail}");
}

#[test]
fn an_edited_old_skill_or_a_foreign_entry_needs_force() {
    let bed = Bed::healthy();
    bed.put_old_setup(
        "---\nname: teamwire\n---\nmine\n",
        r#"{"mcpServers":{"teamwire":{"command":"/bin/other","args":["serve"]}}}"#,
    );
    let detail = detail_of(&bed.checks(), "old lead setup").to_owned();
    assert!(detail.contains("/bin/other serve"), "{detail}");
    assert!(detail.contains("teamwire install --force"), "{detail}");
}

#[test]
fn a_build_under_target_accepts_any_plugin_files_but_still_needs_the_plugin_on() {
    let bed = Bed::healthy();
    let copy = bed.claude.path().join("plugins/cache/teamwire");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join(".mcp.json"), "{}").unwrap();
    bed.register_plugin(&copy, true);
    assert_eq!(level_of(&bed.checks(), "teamwire plugin"), Level::Fail);
    let loose = bed.checks_as(true);
    assert_eq!(
        level_of(&loose, "teamwire plugin"),
        Level::Pass,
        "{loose:?}"
    );
    assert!(detail_of(&loose, "teamwire plugin").contains("not compared"));

    bed.register_plugin(&copy, false);
    assert_eq!(
        level_of(&bed.checks_as(true), "teamwire plugin"),
        Level::Fail
    );
}

#[test]
fn a_project_entry_that_hides_the_user_one_is_a_warning() {
    let bed = Bed::healthy();
    assert_eq!(
        level_of(&bed.checks(), "lead MCP server scope"),
        Level::Pass
    );
    let project = r#"{"mcpServers":{"teamwire":{"command":"teamwire","args":["mcp","--lead"]}}}"#;
    std::fs::write(bed.home.path().join(".mcp.json"), project).unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "lead MCP server scope"), Level::Warn);
    assert!(
        detail_of(&checks, "lead MCP server scope")
            .contains(".mcp.json has `teamwire` = teamwire mcp --lead"),
        "{checks:?}"
    );
}

#[test]
fn the_old_setup_is_checked_even_when_the_entry_cannot_be_worked_out() {
    let bed = Bed::healthy();
    let checks = diagnose(&Inputs {
        host: bed.host,
        home: bed.home.path().to_path_buf(),
        platform: &bed.platform,
        path_var: Some(bed.bin.path().as_os_str().to_owned()),
        claude: bed.claude_config(),
        lead_entry: Err("cannot find this binary".to_owned()),
        running_a_build: false,
        repo: bed.home.path().to_path_buf(),
    });
    assert_eq!(level_of(&checks, "old lead setup"), Level::Pass);
    assert_eq!(
        detail_of(&checks, "teamwire plugin"),
        "cannot find this binary"
    );
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
