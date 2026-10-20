//! `teamwire afk start` through the real binary: each refusal in the order
//! of the design, and one start on a setup where `teamwire doctor` passes.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and git"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::PathBuf;
use std::process::Output;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Clock as _, EventType, HarnessId, HostId, LeadRoute, SessionId, SessionKind, TeamName,
};
use teamwire_fakes::{FakeBins, FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig, McpEntry};
use teamwire_store::{NewTeam, Store};

const SESSION: &str = "11111111-2222-3333-4444-555555555555";

/// A state directory, a Claude directory, a task file, and a repository that
/// ignores `.worktrees/`. `healthy` also makes `teamwire doctor` pass.
struct Bed {
    home: TestHome,
    claude: TestHome,
    work: tempfile::TempDir,
    repo: tempfile::TempDir,
    path: String,
    /// `HERDR_ENV=1` in the command's environment.
    herdr: bool,
    /// The process the session file names as the lead, once `healthy`.
    lead: Option<std::process::Child>,
    /// Further variables for the command's environment.
    env: Vec<(&'static str, &'static str)>,
}

impl Drop for Bed {
    fn drop(&mut self) {
        if let Some(lead) = &mut self.lead {
            drop(lead.kill());
            drop(lead.wait());
        }
    }
}

impl Bed {
    fn new(healthy: bool) -> Self {
        let bed = Self {
            home: TestHome::new().unwrap(),
            claude: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
            repo: tempfile::tempdir().unwrap(),
            path: "/usr/bin:/bin".to_owned(),
            herdr: true,
            lead: None,
            env: Vec::new(),
        };
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(bed.repo.path())
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::write(bed.repo.path().join(".gitignore"), ".worktrees/\n").unwrap();
        std::fs::write(bed.task(), b"build it\n\xff\r\n").unwrap();
        if healthy {
            let mut bed = bed;
            let fakes = bed.work.path().join("fakes");
            FakeBins::ensure().unwrap().install(&fakes).unwrap();
            bed.path = format!("{}:/usr/bin:/bin", fakes.display());
            std::fs::write(
                bed.claude.path().join("settings.json"),
                r#"{"hooks":{"SessionStart":[{"hooks":[{"command":"bash herdr-agent-state.sh session"}]}]}}"#,
            )
            .unwrap();
            bed.install_lead_setup();
            bed.start_lead_process();
            return bed;
        }
        bed
    }

    /// A live process, and the session file that names it as the process of
    /// the lead session: what `afk start` finds to hold the sleep on.
    fn start_lead_process(&mut self) {
        let child = std::process::Command::new("sleep")
            .arg("600")
            .spawn()
            .unwrap();
        let sessions = self.claude.path().join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let file = serde_json::json!({"pid": child.id(), "sessionId": SESSION});
        std::fs::write(
            sessions.join(format!("{}.json", child.id())),
            file.to_string(),
        )
        .unwrap();
        self.lead = Some(child);
    }

    /// Takes the session file away, so the lead's process cannot be found.
    fn forget_lead_process(&self) {
        std::fs::remove_dir_all(self.claude.path().join("sessions")).unwrap();
    }

    fn task(&self) -> PathBuf {
        self.work.path().join("task.md")
    }

    /// The plugin files that `teamwire install` writes, and Claude's own
    /// registration of the plugin.
    fn install_lead_setup(&self) {
        let claude = ClaudeCode::new(
            Arc::new(FakePlatform::new()),
            ClaudeConfig {
                claude_dir: self.claude.path().to_path_buf(),
                claude_json: self.claude.path().join(".claude.json"),
                claude_bin: "claude".to_owned(),
            },
        );
        let bin = std::fs::canonicalize(env!("CARGO_BIN_EXE_teamwire")).unwrap();
        let entry = McpEntry::lead(&bin, Some(self.home.path()));
        let changes = claude
            .plan_install(&entry, self.home.path(), false)
            .unwrap();
        claude.apply_lead_setup(&changes).unwrap();
        let plugins = self.claude.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let root = self.home.path().join("claude-plugin/teamwire");
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
        settings["enabledPlugins"] = serde_json::json!({"teamwire@teamwire": true});
        std::fs::write(path, settings.to_string()).unwrap();
    }

    /// `teamwire afk start` with the given extra arguments, as a lead
    /// session unless `session` is `None`.
    fn start(&self, args: &[&str], session: Option<&str>) -> Output {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
        cmd.arg("--home")
            .arg(self.home.path())
            .args(["afk", "start"])
            .args(args)
            .current_dir(self.repo.path())
            .env_clear()
            .env("PATH", &self.path)
            .env("TEAMWIRE_CLAUDE_DIR", self.claude.path())
            .envs(self.env.iter().copied());
        if self.herdr {
            cmd.env("HERDR_ENV", "1");
        }
        if let Some(session) = session {
            cmd.env("CLAUDE_CODE_SESSION_ID", session);
        }
        cmd.output().unwrap()
    }

    /// A start with the task file and a valid title, plus `args`.
    fn start_run(&self, team: &str, args: &[&str]) -> Output {
        let task = self.task();
        let mut all = vec![
            "--team",
            team,
            "--title",
            "Overnight",
            "--task",
            task.to_str().unwrap(),
        ];
        all.extend_from_slice(args);
        self.start(&all, Some(SESSION))
    }

    fn nothing_made(&self, team: &str) -> bool {
        !self.home.path().join("teams").join(team).exists()
    }

    fn store(&self, team: &str) -> Store {
        Store::open_read_only(self.home.path(), &TeamName::parse(team).unwrap()).unwrap()
    }

    /// A team of `session` in the state directory, AFK when `budget` is set.
    fn seed_team(&self, team: &str, session: &str, budget: Option<Duration>) {
        let name = TeamName::parse(team).unwrap();
        let mut store = Store::open(self.home.path(), &name).unwrap();
        let clock = FakeClock::new();
        let new = NewTeam {
            name: name.clone(),
            title: "Seed".to_owned(),
            lead_harness: HarnessId::from_static(FakeHarness::ID),
            lead_host: HostId::from_static(FakeHost::ID),
            lead_route: LeadRoute::Sink,
            lead_session: Some((SessionId::parse(session).unwrap(), SessionKind::Interactive)),
        };
        store
            .write(|tx| match budget {
                Some(b) => tx
                    .create_afk_team(
                        clock.now(),
                        &new,
                        clock.now().checked_add(b).unwrap(),
                        &serde_json::json!({}),
                    )
                    .map(|_| ()),
                None => tx.create_team(clock.now(), &new).map(|_| ()),
            })
            .unwrap();
    }
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn refused(out: &Output, code: &str) {
    assert_eq!(out.status.code(), Some(1), "{}", err(out));
    assert!(
        err(out).contains(&format!("refused: {code}:")),
        "{}",
        err(out)
    );
}

#[test]
fn outside_a_lead_session_it_is_refused() {
    let bed = Bed::new(false);
    let task = bed.task();
    let out = bed.start(
        &[
            "--team",
            "run",
            "--title",
            "T",
            "--task",
            task.to_str().unwrap(),
        ],
        None,
    );
    refused(&out, "no_lead_session");
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_member_session_is_refused_before_anything_is_made() {
    let mut bed = Bed::new(false);
    bed.env = vec![("TEAMWIRE_TEAM", "alpha"), ("TEAMWIRE_ROLE", "api")];
    let out = bed.start_run("run", &[]);
    assert_eq!(out.status.code(), Some(1), "{}", err(&out));
    assert!(err(&out).contains("refused.identity"), "{}", err(&out));
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_lead_that_is_not_claude_code_is_refused_before_anything_is_made() {
    let mut bed = Bed::new(true);
    bed.env = vec![("TEAMWIRE_LEAD_HARNESS", "codex")];
    let out = bed.start_run("run", &[]);
    assert_eq!(out.status.code(), Some(1), "{}", err(&out));
    assert!(
        err(&out).contains("unsupported_lead_harness"),
        "{}",
        err(&out)
    );
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_session_that_already_leads_an_open_afk_team_is_refused() {
    let bed = Bed::new(false);
    bed.seed_team("first", SESSION, Some(Duration::from_secs(3600)));
    let out = bed.start_run("second", &[]);
    refused(&out, "afk_already_running");
    assert!(err(&out).contains("first"), "{}", err(&out));
    assert!(bed.nothing_made("second"));
}

#[test]
fn a_bad_or_taken_name_is_refused() {
    let bed = Bed::new(false);
    refused(&bed.start_run("my-run", &[]), "bad_name");
    // An ordinary team of the same session is not an AFK run, and holds the name.
    bed.seed_team("taken", SESSION, None);
    refused(&bed.start_run("taken", &[]), "team_exists");
}

#[test]
fn an_empty_or_long_title_is_refused() {
    let bed = Bed::new(false);
    let task = bed.task();
    let task = task.to_str().unwrap();
    let long = "x".repeat(29);
    for (title, code) in [("  ", "title_missing"), (long.as_str(), "title_too_long")] {
        let out = bed.start(
            &["--team", "run", "--title", title, "--task", task],
            Some(SESSION),
        );
        refused(&out, code);
    }
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_missing_task_file_is_refused_before_anything_is_made() {
    let bed = Bed::new(false);
    let out = bed.start(
        &[
            "--team",
            "run",
            "--title",
            "T",
            "--task",
            "/no/such/task.md",
        ],
        Some(SESSION),
    );
    refused(&out, "bad_task");
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_bad_budget_is_a_usage_error() {
    let bed = Bed::new(false);
    for budget in ["0h", "8", "8d", "h", "1.5h", ""] {
        let out = bed.start_run("run", &["--budget", budget]);
        assert_eq!(out.status.code(), Some(3), "{budget}: {}", err(&out));
    }
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_budget_that_ends_beyond_the_last_date_is_refused_before_anything_is_made() {
    let bed = Bed::new(true);
    refused(
        &bed.start_run("run", &["--budget", "100000000h"]),
        "bad_budget",
    );
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_failing_doctor_check_refuses_and_names_it() {
    let bed = Bed::new(false);
    let out = bed.start_run("run", &[]);
    refused(&out, "doctor_failed");
    assert!(err(&out).contains("FAIL claude on PATH"), "{}", err(&out));
    assert!(!err(&out).contains("break a launch"), "{}", err(&out));
    assert!(bed.nothing_made("run"));
}

#[test]
fn a_start_makes_the_team_with_its_budget_and_a_copy_of_the_task() {
    let bed = Bed::new(true);
    // An ordinary team of the same session does not block an AFK run.
    bed.seed_team("plain", SESSION, None);
    let out = bed.start_run("run", &[]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    let shown = String::from_utf8_lossy(&out.stdout).into_owned();
    let dir = bed.home.path().join("teams/run/afk");
    assert!(shown.contains("afk run run is open"), "{shown}");
    assert!(
        shown.contains(&format!("folder: {}", dir.display())),
        "{shown}"
    );
    assert!(
        shown.contains(&format!("report: {}", dir.join("REPORT.md").display())),
        "{shown}"
    );
    assert!(shown.contains("keep the lid open"), "{shown}");

    assert_eq!(
        std::fs::read(dir.join("task.md")).unwrap(),
        std::fs::read(bed.task()).unwrap()
    );
    let store = bed.store("run");
    let row = store.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(row.title, "Overnight");
    assert_eq!(row.lead_session, Some(SessionId::parse(SESSION).unwrap()));
    // The hold runs on the lead's process wherever the platform has one.
    assert_eq!(row.sleep_hold.is_some(), cfg!(target_os = "macos"));
    let ends = row.afk_budget_ends_at.unwrap();
    let left = ends.saturating_since(row.created_at);
    assert!(
        left <= Duration::from_secs(8 * 3600) && left > Duration::from_secs(8 * 3600 - 60),
        "{left:?}"
    );
    let created = store
        .read(|tx| tx.events_of_type(EventType::TeamCreated)?.all_or_corrupt())
        .unwrap();
    assert_eq!(created[0].data["afk_budget_ends_at"], ends.to_string());

    // The same session cannot start a second run, and a second run by
    // another session cannot reuse the name.
    refused(&bed.start_run("again", &[]), "afk_already_running");
}

#[test]
fn the_budget_option_sets_the_end() {
    let bed = Bed::new(true);
    let out = bed.start_run("run", &["--budget", "90m"]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    let row = bed.store("run").read(|tx| tx.team()).unwrap().unwrap();
    let left = row
        .afk_budget_ends_at
        .unwrap()
        .saturating_since(row.created_at);
    assert!(
        left <= Duration::from_secs(90 * 60) && left > Duration::from_secs(89 * 60),
        "{left:?}"
    );
}

#[test]
fn a_start_without_a_host_is_refused_before_anything_is_made() {
    let mut bed = Bed::new(true);
    bed.herdr = false;
    refused(&bed.start_run("run", &[]), "no_host");
    assert!(bed.nothing_made("run"));
}

/// Only a platform with a sleep hold needs the lead's process.
#[cfg(target_os = "macos")]
#[test]
fn a_start_that_cannot_find_the_leads_process_is_refused_before_anything_is_made() {
    let bed = Bed::new(true);
    bed.forget_lead_process();
    refused(&bed.start_run("run", &[]), "no_lead_process");
    assert!(bed.nothing_made("run"));
}

/// A store file in `dir` with the schema and a trigger that fails the insert
/// of the team row with an `io` error.
fn store_that_cannot_take_a_team(dir: &std::path::Path, team: &str) -> PathBuf {
    let prep = TestHome::new().unwrap();
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(prep.path(), &name).unwrap();
    store
        .write(|tx| {
            tx.connection()
                .execute_batch(
                    "CREATE TRIGGER no_team BEFORE INSERT ON team
                     BEGIN SELECT no_such_function(); END",
                )
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    drop(store);
    let path = dir.join("prepared.db");
    std::fs::copy(teamwire_store::database_path(prep.path(), &name), &path).unwrap();
    path
}

/// The store is made after the start looked for one and before it opens
/// one: the task file is a named pipe, the start waits on it, and the test
/// puts the store in place before it lets the task through.
#[test]
fn a_start_that_fails_in_the_store_leaves_one_store_error_row() {
    let bed = Bed::new(true);
    let fifo = bed.work.path().join("task.fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    let team = TeamName::parse("run").unwrap();
    let prepared = store_that_cannot_take_a_team(bed.work.path(), "run");
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(bed.home.path())
        .args([
            "afk",
            "start",
            "--team",
            "run",
            "--title",
            "Overnight",
            "--task",
        ])
        .arg(&fifo)
        .current_dir(bed.repo.path())
        .env_clear()
        .env("PATH", &bed.path)
        .env("TEAMWIRE_CLAUDE_DIR", bed.claude.path())
        .env("HERDR_ENV", "1")
        .env("CLAUDE_CODE_SESSION_ID", SESSION)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn().unwrap();
    // Opens only once the start reads the task file.
    let mut writer = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();
    let db = teamwire_store::database_path(bed.home.path(), &team);
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    std::fs::copy(&prepared, &db).unwrap();
    std::io::Write::write_all(&mut writer, b"build it\n").unwrap();
    drop(writer);
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", err(&out));
    let errors: Vec<serde_json::Value> = bed
        .store("run")
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventType::StoreError)
        .map(|e| e.data)
        .collect();
    assert_eq!(errors, [serde_json::json!({ "op": "afk", "code": "io" })]);
}
