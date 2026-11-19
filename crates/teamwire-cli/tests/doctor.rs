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
use std::sync::Arc;
use std::time::Duration;
use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    Clock, HarnessId, HostId, LeadCommand, LeadRoute, MemberKind, MemberState, Mode, Pid,
    PlatformError, PlatformErrorCode, Registry, Role, RunOutput, TeamName, Timestamp,
};
use teamwire_fakes::{FakeBins, FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig, McpEntry};
use teamwire_store::{NewTeam, Store};

#[path = "support/schema_1.rs"]
mod schema_1;

#[path = "doctor/speckit.rs"]
mod speckit;

/// The command file is compiled here with the crate-level items it names
/// (`Global`, `wiring::Wiring`, `cmd::shared::home_or_refuse`,
/// `cmd::install::wanted_command`) and the real `cmd::lead_setup_check`,
/// `cmd::host_pick`, `cmd::doctor_speckit` and `cmd::doctor_teams`, so the
/// checks run against a fake platform.
#[derive(Debug)]
pub(crate) struct Global {
    pub(crate) home: Option<PathBuf>,
}

mod wiring {
    use std::sync::Arc;

    use teamwire_core::Platform;
    pub(crate) use teamwire_harness_claude::{ClaudeConfig, SKILL_TEXT};
    pub(crate) use teamwire_harness_pi::PiConfig;

    /// The tests pass the setting in `Inputs`; the real reader is not compiled here.
    pub(crate) fn lead_handoff_setting(
        _home: &std::path::Path,
        _env: &dyn Fn(&str) -> Option<String>,
    ) -> teamwire_core::LeadHandoffSetting {
        teamwire_core::LeadHandoffSetting::Unset
    }

    pub(crate) struct Wiring {
        pub(crate) platform: Arc<dyn Platform>,
        pub(crate) claude: ClaudeConfig,
        pub(crate) pi: PiConfig,
        pub(crate) registry: teamwire_core::Registry,
        pub(crate) clock: teamwire_fakes::FakeClock,
    }
}

#[path = "../src/cmd/lead_setup_check.rs"]
mod lead_setup_check;

#[path = "../src/cmd/host_pick.rs"]
mod host_pick;

#[path = "../src/cmd/doctor_speckit.rs"]
mod doctor_speckit;

#[path = "../src/cmd/doctor_speckit_files.rs"]
mod doctor_speckit_files;

#[path = "../src/cmd/doctor_teams.rs"]
mod doctor_teams;

mod cmd {
    pub(crate) use crate::{
        doctor, doctor_speckit, doctor_speckit_files, doctor_teams, host_pick, lead_setup_check,
    };

    pub(crate) mod install {
        use teamwire_core::LeadCommand;

        use crate::Global;

        /// The tests pass the wanted command in `Inputs`; the real helper is not compiled here.
        pub(crate) fn wanted_command(_global: &Global) -> Result<LeadCommand, String> {
            Err("not compiled in this test".to_owned())
        }
    }

    pub(crate) mod shared {
        use std::path::PathBuf;
        use std::process::ExitCode;

        use crate::Global;

        /// The exit code of a run that found a problem; same value as the real one.
        pub(crate) const PARTIAL: u8 = 2;

        /// Prints nothing; the tests read the report text, not the output.
        pub(crate) fn say(_text: &str) {}

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
    /// What `claude --help` prints.
    claude_help: String,
    /// The environment variables that are set.
    env_names: Vec<String>,
    /// What `TEAMWIRE_LEAD_HANDOFF_TOKENS` says.
    lead_handoff: teamwire_core::LeadHandoffSetting,
    registry: Registry,
    clock: FakeClock,
    /// The state directory of the doctor run, when it is not `home`, the one
    /// the plugin was installed for.
    run_home: Option<PathBuf>,
    /// The git the spec-kit checks ask: a real one, unless a test gives a
    /// fake one with its answers queued.
    git: Option<Arc<FakePlatform>>,
    /// What `--base` names.
    base: String,
}

const ALL_FLAGS: &str = "--session-id --resume --permission-mode --append-system-prompt-file \
                         --settings --mcp-config --model --effort";

fn answer(text: &str) -> RunOutput {
    RunOutput {
        exit_code: 0,
        stdout: text.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out: false,
    }
}

impl Bed {
    /// A setup on which every check passes.
    fn healthy() -> Self {
        let mut bed = Self {
            home: TestHome::new().unwrap(),
            claude: TestHome::new().unwrap(),
            bin: TestHome::new().unwrap(),
            platform: FakePlatform::new(),
            host: Host::Herdr,
            claude_help: ALL_FLAGS.to_owned(),
            env_names: Vec::new(),
            lead_handoff: teamwire_core::LeadHandoffSetting::Unset,
            registry: Registry::new(),
            clock: FakeClock::new(),
            run_home: None,
            git: None,
            base: "HEAD".to_owned(),
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
        bed.registry = bed.registry();
        bed
    }

    /// The fake harness the teams run on, and the two real harnesses whose
    /// programs and lead setups doctor checks, each on a platform of its own
    /// so their commands do not mix with doctor's probes.
    fn registry(&self) -> Registry {
        let mut registry = Registry::new();
        registry.add_harness(Arc::new(FakeHarness::new())).unwrap();
        registry
            .add_harness(Arc::new(ClaudeCode::new(
                Arc::new(FakePlatform::new()),
                self.claude_config(),
            )))
            .unwrap();
        registry.add_harness(Arc::new(self.pi_setup())).unwrap();
        registry.add_host(Arc::new(FakeHost::new())).unwrap();
        registry
    }

    fn pi_config(&self) -> teamwire_harness_pi::PiConfig {
        teamwire_harness_pi::PiConfig {
            agent_dir: self.home.path().join("pi-agent"),
            pi_bin: "pi".to_owned(),
            exclude_providers: Vec::new(),
        }
    }

    /// The command the teamwire binary under test would give each lead
    /// setup for this bed's state directory.
    fn lead_command(&self) -> LeadCommand {
        LeadCommand {
            bin: std::fs::canonicalize(env!("CARGO_BIN_EXE_teamwire")).unwrap(),
            home: Some(
                self.run_home
                    .clone()
                    .unwrap_or_else(|| self.home.path().to_path_buf()),
            ),
        }
    }

    /// The pi adapter, on a platform of its own: the pi check reads files
    /// and runs nothing.
    fn pi_setup(&self) -> teamwire_harness_pi::Pi {
        teamwire_harness_pi::Pi::new(Arc::new(FakePlatform::new()), self.pi_config())
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
        McpEntry::lead(
            &bin,
            Some(self.run_home.as_deref().unwrap_or(self.home.path())),
        )
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
        // The order doctor runs them in: git, then the claude probes, then
        // the host's version. The spec-kit checks ask their own git.
        self.platform.queue_run(Ok(answer("")));
        self.platform.queue_run(Ok(answer("2.1.0 (Claude Code)\n")));
        self.platform.queue_run(Ok(answer(&self.claude_help)));
        self.platform.queue_run(Ok(answer("host 1.2.3\n")));
        let real_git = teamwire_platform::native();
        let git: &dyn teamwire_core::Platform = match &self.git {
            Some(fake) => fake.as_ref(),
            None => real_git.as_ref(),
        };
        diagnose(&Inputs {
            pi: self.pi_config(),
            host: self.host,
            home: self
                .run_home
                .clone()
                .unwrap_or_else(|| self.home.path().to_path_buf()),
            platform: &self.platform,
            registry: &self.registry,
            clock: &self.clock,
            path_var: Some(self.bin.path().as_os_str().to_owned()),
            claude: self.claude_config(),
            lead_command: Ok(self.lead_command()),
            running_a_build: build,
            repo: self.home.path().to_path_buf(),
            base: self.base.clone(),
            git,
            env_names: self.env_names.clone(),
            lead_handoff: self.lead_handoff.clone(),
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

/// Runs the statement a counters line prints after "repair with: ".
fn apply_repair(store: &mut Store, text: &str) {
    let sql = text.split_once("repair with: ").unwrap().1.to_owned();
    store
        .write(|tx| Ok(tx.connection().execute(&sql, ())?))
        .unwrap();
}

#[test]
fn a_message_counter_below_an_id_in_use_fails_and_prints_the_repair() {
    let bed = Bed::healthy();
    let mut store = bed.team("alpha");
    let send = |store: &mut Store| {
        store
            .write(|tx| {
                tx.insert_message(
                    FakeClock::new().now(),
                    &teamwire_store::NewMessage {
                        sender: teamwire_core::Actor::Lead,
                        kind: teamwire_core::MessageType::Note,
                        re: None,
                        commit: None,
                        body: "x".to_owned(),
                        client_key: None,
                        recipients: vec![teamwire_core::Recipient::Lead],
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    };
    send(&mut store);
    send(&mut store);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha counters"), Level::Pass);
    // Damaged by raw SQL, as the store's corrupt test does it.
    store
        .write(|tx| {
            Ok(tx.connection().execute(
                "UPDATE counters SET value = 1 WHERE name = 'display_id'",
                (),
            )?)
        })
        .unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha counters"), Level::Fail);
    let text = detail_of(&checks, "team alpha counters");
    assert!(
        text.contains("display_id is 1 but 2 is in use")
            && text.contains("INSERT INTO counters (name, value) VALUES ('display_id', 2)"),
        "{text}"
    );
    // Doctor only prints the repair: the row is still damaged.
    assert_eq!(store.read(|tx| tx.counter("display_id")).unwrap(), Some(1));
    apply_repair(&mut store, text);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha counters"), Level::Pass);

    // A deleted row is reported as missing, and the same kind of statement fixes it.
    store
        .write(|tx| {
            Ok(tx
                .connection()
                .execute("DELETE FROM counters WHERE name = 'display_id'", ())?)
        })
        .unwrap();
    let checks = bed.checks();
    let text = detail_of(&checks, "team alpha counters");
    assert!(
        text.contains("display_id is missing but 2 is in use"),
        "{text}"
    );
    apply_repair(&mut store, text);
    assert_eq!(level_of(&bed.checks(), "team alpha counters"), Level::Pass);
}

#[test]
fn a_database_of_another_schema_version_fails() {
    let bed = Bed::healthy();
    drop(bed.team("alpha"));
    let path = teamwire_store::database_path(bed.home.path(), &TeamName::parse("alpha").unwrap());
    // `PRAGMA user_version` is bytes 60..64 of the database header.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[60..64].copy_from_slice(&(teamwire_store::SCHEMA_VERSION + 1).to_be_bytes());
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
fn a_watch_lease_with_a_live_pid_and_an_old_heartbeat_warns() {
    let bed = Bed::healthy();
    let mut store = bed.team("alpha");
    let pid = Pid::new(4242);
    let now = bed.clock.now();
    store
        .write(|tx| tx.take_watch_lease(now, pid, "herdr", |_| true).map(|_| ()))
        .unwrap();
    drop(store);
    bed.platform.set_alive(pid, true);

    let checks = bed.checks();
    assert!(
        checks.iter().all(|c| c.name != "team alpha watch lease"),
        "a fresh lease is not stale: {checks:?}"
    );

    bed.clock.advance(Duration::from_secs(601));
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha watch lease"), Level::Warn);
    let detail = detail_of(&checks, "team alpha watch lease");
    assert!(detail.contains("has not beaten"), "{detail}");
}

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
fn herdrs_pi_integration_is_checked_only_when_pi_is_on_path() {
    let bed = Bed::healthy();
    assert!(!names(&bed.checks()).contains(&"herdr pi integration"));
    std::fs::write(bed.bin.path().join("pi"), "#!/bin/sh\n").unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "herdr pi integration"), Level::Warn);
    assert!(detail_of(&checks, "herdr pi integration").contains("herdr integration install pi"));
    let ext = bed.home.path().join("pi-agent/extensions");
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(ext.join("herdr-agent-state.ts"), "").unwrap();
    assert_eq!(level_of(&bed.checks(), "herdr pi integration"), Level::Pass);
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
fn a_plugin_installed_for_another_home_warns_naming_both_and_gives_no_install_advice() {
    let mut bed = Bed::healthy();
    let other = TestHome::new().unwrap();
    bed.run_home = Some(other.path().to_path_buf());
    let checks = bed.checks();
    assert_eq!(
        level_of(&checks, "teamwire plugin"),
        Level::Warn,
        "{checks:?}"
    );
    let detail = detail_of(&checks, "teamwire plugin");
    assert_eq!(
        detail,
        format!(
            "installed for state directory {}; this run uses {}",
            bed.home.path().display(),
            other.path().display()
        )
    );
    assert!(!detail.contains("install`"), "{detail}");
    assert!(checks.iter().all(|c| c.level != Level::Fail), "{checks:?}");
}

#[test]
fn a_plugin_that_differs_in_more_than_its_home_still_fails_in_another_home() {
    let mut bed = Bed::healthy();
    let other = TestHome::new().unwrap();
    bed.run_home = Some(other.path().to_path_buf());
    std::fs::write(bed.plugin_root().join("hooks/hooks.json"), "{}").unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), ["teamwire plugin"]);
    assert!(detail_of(&checks, "teamwire plugin").contains("another version"));
}

#[test]
fn a_missing_or_changed_afk_skill_fails_like_the_main_skill() {
    let bed = Bed::healthy();
    let afk = bed.plugin_root().join("skills/afk/SKILL.md");
    std::fs::write(&afk, "changed").unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), ["teamwire plugin"]);
    assert!(detail_of(&checks, "teamwire plugin").contains("another version"));

    std::fs::remove_file(&afk).unwrap();
    let checks = bed.checks();
    assert_eq!(failing(&checks), ["teamwire plugin"]);
    assert!(detail_of(&checks, "teamwire plugin").contains("run `teamwire install`"));
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
        registry: &bed.registry,
        clock: &bed.clock,
        path_var: Some(bed.bin.path().as_os_str().to_owned()),
        claude: bed.claude_config(),
        pi: bed.pi_config(),
        lead_command: Err("cannot find this binary".to_owned()),
        running_a_build: false,
        repo: bed.home.path().to_path_buf(),
        base: "HEAD".to_owned(),
        git: &bed.platform,
        env_names: Vec::new(),
        lead_handoff: teamwire_core::LeadHandoffSetting::Unset,
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
    teamwire_with(bed, path, cwd, &[])
}

/// `teamwire doctor` with `extra` arguments after `doctor`.
fn teamwire_with(bed: &Bed, path: &str, cwd: &Path, extra: &[&str]) -> Run {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .arg("doctor")
        .args(extra)
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

#[test]
fn the_versions_of_claude_and_the_host_are_shown_on_the_pass_lines() {
    let bed = Bed::healthy();
    let checks = bed.checks();
    let detail = |name: &str| {
        checks
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .detail
            .clone()
    };
    assert_eq!(detail("claude version"), "2.1.0 (Claude Code)");
    assert_eq!(detail("herdr version"), "host 1.2.3");
    assert_eq!(level_of(&checks, "claude flags"), Level::Pass);
}

#[test]
fn a_claude_help_without_a_launch_flag_warns_and_names_it() {
    let mut bed = Bed::healthy();
    bed.claude_help = ALL_FLAGS.replace("--effort", "");
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "claude flags"), Level::Warn);
    let detail = &checks
        .iter()
        .find(|c| c.name == "claude flags")
        .unwrap()
        .detail;
    assert!(
        detail.contains("--effort") && !detail.contains("--model"),
        "{detail}"
    );
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn a_probe_that_times_out_or_fails_is_a_warning() {
    let bed = Bed::healthy();
    bed.platform.queue_run(Ok(answer("")));
    bed.platform.queue_run(Ok(RunOutput {
        exit_code: 124,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: true,
    }));
    bed.platform
        .queue_run(Err(PlatformError::new(PlatformErrorCode::NotFound, "gone")));
    // These queue ahead of `checks_as`'s own answers: git, the claude
    // version, the claude help.
    let checks = bed.checks();
    assert_eq!(
        level_of(&checks, "claude version"),
        Level::Warn,
        "{checks:?}"
    );
    assert_eq!(level_of(&checks, "claude flags"), Level::Warn, "{checks:?}");
}

#[test]
fn a_credential_variable_warns_by_name_and_a_clean_environment_passes() {
    let mut bed = Bed::healthy();
    assert_eq!(level_of(&bed.checks(), "credential variables"), Level::Pass);
    bed.env_names = ["ANTHROPIC_API_KEY", "CLAUDE_CODE_USE_VERTEX", "HOME"]
        .map(str::to_owned)
        .to_vec();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "credential variables"), Level::Warn);
    let detail = &checks
        .iter()
        .find(|c| c.name == "credential variables")
        .unwrap()
        .detail;
    assert!(
        detail.contains("ANTHROPIC_API_KEY, CLAUDE_CODE_USE_VERTEX"),
        "{detail}"
    );
    assert!(!detail.contains("HOME"), "{detail}");
}

/// Starts member `role` on `store`, which writes `version` as the writer of
/// the `member.started` event.
fn start_member(store: &mut Store, role: &str) {
    let role = Role::parse(role).unwrap();
    let now = FakeClock::new().now();
    let new = teamwire_store::NewMember {
        role: role.clone(),
        title: "T".to_owned(),
        kind: MemberKind::Implementer,
        mode: Mode::Pane,
        harness: HarnessId::from_static(FakeHarness::ID),
        host: HostId::from_static(FakeHost::ID),
        cwd: PathBuf::from("/tmp"),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: vec![],
        deadline_s: 1800,
    };
    store
        .write(|tx| {
            tx.insert_member(now, &new)?;
            tx.set_member_state(now, &role, MemberState::Started, &serde_json::json!({}))
        })
        .unwrap();
}

#[test]
fn an_open_team_with_a_member_from_another_build_warns_and_names_the_role_and_fix() {
    let bed = Bed::healthy();
    let mut store = bed.team("alpha");
    start_member(&mut store, "api");
    drop(store);
    let mut older = Store::open(bed.home.path(), &TeamName::parse("alpha").unwrap())
        .unwrap()
        .with_writer_version("0.0.0+aaaaaaaaaaaa");
    start_member(&mut older, "web");
    drop(older);

    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha older members"), Level::Warn);
    let detail = &checks
        .iter()
        .find(|c| c.name == "team alpha older members")
        .unwrap()
        .detail;
    assert!(
        detail.contains("web (0.0.0+aaaaaaaaaaaa)") && !detail.contains("api"),
        "{detail}"
    );
    assert!(
        detail.contains("relaunch the member, or ignore if its wiring did not change"),
        "{detail}"
    );
    assert_eq!(exit_code(&checks), 2);
}

#[test]
fn members_of_this_build_pass_the_older_members_check() {
    let bed = Bed::healthy();
    let mut store = bed.team("alpha");
    start_member(&mut store, "api");
    drop(store);
    assert_eq!(
        level_of(&bed.checks(), "team alpha older members"),
        Level::Pass
    );
}

#[test]
fn a_member_from_another_build_that_has_finished_is_not_counted() {
    let bed = Bed::healthy();
    let mut older = bed.team("alpha").with_writer_version("0.0.0");
    start_member(&mut older, "web");
    start_member(&mut older, "api");
    complete(&mut older, "web");
    // The watch delivered the result, so the lead has it.
    let id = older
        .read(|tx| {
            tx.completions_of(&Role::parse("web").unwrap())?
                .all_or_corrupt()
        })
        .unwrap()[0]
        .completion
        .ulid;
    older
        .write(|tx| tx.deliver_completion_via_watch(FakeClock::new().now(), &id))
        .unwrap();
    drop(older);

    let checks = bed.checks();
    let detail = &checks
        .iter()
        .find(|c| c.name == "team alpha older members")
        .unwrap()
        .detail;
    assert!(
        detail.contains("api (0.0.0)") && !detail.contains("web"),
        "{detail}"
    );
    assert!(detail.contains("shows as 0.0.0"), "{detail}");
}

fn complete(store: &mut Store, role: &str) {
    let now = FakeClock::new().now();
    store
        .write(|tx| {
            tx.insert_completion(
                now,
                &teamwire_store::NewCompletion {
                    role: Role::parse(role).unwrap(),
                    generation: 1,
                    status: teamwire_core::CompletionStatus::Done,
                    summary: "finished".to_owned(),
                    final_message: None,
                    final_source: teamwire_core::FinalSource::Hook,
                },
            )
            .map(|_| ())
        })
        .unwrap();
}

/// The watch's UNREPORTED line sends the lead here, so the member whose
/// result nobody has received must be listed.
#[test]
fn a_member_from_another_build_whose_result_is_pending_is_counted() {
    let bed = Bed::healthy();
    let mut older = bed.team("alpha").with_writer_version("0.0.0");
    start_member(&mut older, "web");
    complete(&mut older, "web");
    drop(older);

    let checks = bed.checks();
    assert_eq!(level_of(&checks, "team alpha older members"), Level::Warn);
    assert!(detail_of(&checks, "team alpha older members").contains("web (0.0.0)"));
}

#[test]
fn pi_on_path_has_its_lead_package_checked() {
    use teamwire_harness_pi::lead_setup::{Change, LeadCommand, package_dir};
    let bed = Bed::healthy();
    assert!(!names(&bed.checks()).contains(&"pi lead package"));
    std::fs::write(bed.bin.path().join("pi"), "#!/bin/sh\n").unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "pi lead package"), Level::Pass);
    let detail = detail_of(&checks, "pi lead package");
    assert!(detail.contains("teamwire install --harness pi"), "{detail}");

    // Install as `teamwire install` would, from this test binary's entry.
    let entry = bed.lead_entry();
    let want = LeadCommand {
        bin: PathBuf::from(&entry.command),
        home: Some(bed.home.path().to_path_buf()),
    };
    let pi = bed.pi_setup();
    let changes = pi
        .plan_install(&want, teamwire_harness_claude::SKILL_TEXT, bed.home.path())
        .unwrap();
    let writes: Vec<_> = changes
        .into_iter()
        .filter(|c| matches!(c, Change::WritePackage { .. }))
        .collect();
    pi.apply_lead_setup(&writes).unwrap();
    let dir = package_dir(bed.home.path());
    std::fs::create_dir_all(bed.home.path().join("pi-agent")).unwrap();
    std::fs::write(
        bed.home.path().join("pi-agent/settings.json"),
        serde_json::json!({ "packages": [dir.display().to_string()] }).to_string(),
    )
    .unwrap();
    assert_eq!(detail_of(&bed.checks(), "pi lead package"), "installed");

    // A file another binary wrote is a warning that names the fix.
    std::fs::write(dir.join("extensions/teamwire-lead.ts"), "// old").unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "pi lead package"), Level::Warn);
    assert!(detail_of(&checks, "pi lead package").contains("teamwire install --harness pi"));
}

/// ADR 0051 decision 6: a lead handoff setting that is not a whole number or
/// `off`, or one below the floor, is a warning that names the value. Any
/// other setting prints no line.
#[test]
fn a_bad_or_low_lead_handoff_setting_is_a_warning() {
    use teamwire_core::LeadHandoffSetting;
    let mut bed = Bed::healthy();
    bed.lead_handoff = LeadHandoffSetting::parse(Some("40%"));
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "lead handoff"), Level::Warn);
    assert!(
        detail_of(&checks, "lead handoff").starts_with(
            "TEAMWIRE_LEAD_HANDOFF_TOKENS is \"40%\", which is neither a whole number of tokens nor off"
        ),
        "{checks:?}"
    );
    bed.lead_handoff = LeadHandoffSetting::Tokens(0);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "lead handoff"), Level::Warn);
    assert!(
        detail_of(&checks, "lead handoff").starts_with(
            "TEAMWIRE_LEAD_HANDOFF_TOKENS is 0, below the floor of 200000, so it counts as 200000"
        ),
        "{checks:?}"
    );
    for fine in [
        LeadHandoffSetting::Unset,
        LeadHandoffSetting::Off,
        LeadHandoffSetting::Tokens(200_000),
    ] {
        bed.lead_handoff = fine.clone();
        let checks = bed.checks();
        assert!(
            !checks.iter().any(|c| c.name == "lead handoff"),
            "{fine:?}: {checks:?}"
        );
    }
}
