//! Shared by the integration tests: a real git repository in a temporary
//! directory, a team store, and a `Platform` that runs real processes so the
//! tests exercise the git and shell calls verify makes.
#![expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "test support: real git repositories need real processes, and a failure here fails the test"
)]

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Command, Config, HarnessId, HostId, LeadRoute, Member, MemberKind, MemberState, Mode, Pid,
    Platform, PlatformError, Registry, Role, RunOutput, SleepHold, TeamName,
};
use teamwire_fakes::{FakeClock, FakePlatform, TestHome};
use teamwire_store::{Ctx, NewMember, NewTeam, Store};

/// Runs real processes for `run`, unless the test scripted the next answer;
/// everything else is the in-process fake.
#[derive(Debug)]
pub struct TestPlatform {
    inner: FakePlatform,
    runs: Mutex<Vec<(Command, Duration)>>,
    scripted: Mutex<VecDeque<(String, RunOutput)>>,
}

impl TestPlatform {
    fn new() -> Self {
        Self {
            inner: FakePlatform::new(),
            runs: Mutex::new(Vec::new()),
            scripted: Mutex::new(VecDeque::new()),
        }
    }

    /// Every command passed to `run`, with its timeout, in order.
    pub fn commands(&self) -> Vec<(Command, Duration)> {
        self.runs.lock().unwrap().clone()
    }

    /// Paths `write_atomic` wrote, in order.
    pub fn written(&self) -> Vec<PathBuf> {
        self.inner.written()
    }

    /// Answers the next `run` of `program` with `out` instead of starting it.
    pub fn script_next(&self, program: &str, out: RunOutput) {
        self.scripted
            .lock()
            .unwrap()
            .push_back((program.to_owned(), out));
    }
}

fn real(cmd: &Command) -> Result<RunOutput, PlatformError> {
    let mut process = std::process::Command::new(&cmd.program);
    process
        .args(&cmd.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = &cmd.cwd {
        process.current_dir(dir);
    }
    for (key, value) in &cmd.env {
        process.env(key, value);
    }
    let mut child = process
        .spawn()
        .map_err(|e| PlatformError::new(teamwire_core::PlatformErrorCode::Io, e.to_string()))?;
    drop(
        child
            .stdin
            .take()
            .map(|mut s| s.write_all(&cmd.stdin.clone().unwrap_or_default())),
    );
    let out = child
        .wait_with_output()
        .map_err(|e| PlatformError::new(teamwire_core::PlatformErrorCode::Io, e.to_string()))?;
    Ok(RunOutput {
        exit_code: out.status.code().unwrap_or(-1),
        stdout: out.stdout,
        stderr: out.stderr,
        timed_out: false,
    })
}

impl Platform for TestPlatform {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }

    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }

    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        self.runs.lock().unwrap().push((cmd.clone(), timeout));
        let mut scripted = self.scripted.lock().unwrap();
        if let Some(at) = scripted
            .iter()
            .position(|(program, _)| *program == cmd.program)
        {
            return Ok(scripted.remove(at).unwrap().1);
        }
        drop(scripted);
        real(cmd)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
        self.inner.write_atomic(path, bytes)
    }

    fn sleep_hold(&self) -> Option<&dyn SleepHold> {
        self.inner.sleep_hold()
    }

    fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError> {
        self.inner.free_bytes(path)
    }

    fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError> {
        self.inner.on_network_fs(path)
    }
}

/// A git repository in a temporary directory.
#[derive(Debug)]
pub struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    /// `main` with one commit that holds `files`, authored at `at` (unix seconds).
    pub fn init(files: &[(&str, &str)], at: i64) -> Self {
        let repo = Self {
            dir: tempfile::Builder::new()
                .prefix("teamwire-verify-repo-")
                .tempdir()
                .unwrap(),
        };
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.commit(files, "initial", at);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Runs git in the repository; panics unless it exits 0. Returns stdout.
    pub fn git(&self, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(self.path())
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    /// Writes `files` and commits them, authored and committed at `at`.
    pub fn commit(&self, files: &[(&str, &str)], message: &str, at: i64) -> String {
        self.write(files);
        self.git(&["add", "-A", "--", "."]);
        self.git_at(&["commit", "--quiet", "-m", message], at)
    }

    /// Like `git`, with author and committer time fixed at `at`.
    pub fn git_at(&self, args: &[&str], at: i64) -> String {
        let date = format!("{at} +0000");
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(self.path())
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        self.git(&["rev-parse", "HEAD"])
    }

    /// Writes files without committing them.
    pub fn write(&self, files: &[(&str, &str)]) {
        for (path, content) in files {
            let full = self.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, content).unwrap();
        }
    }

    pub fn remove(&self, path: &str) {
        std::fs::remove_file(self.path().join(path)).unwrap();
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
}

/// What a test says about the one member it adds.
#[derive(Debug)]
pub struct MemberSetup<'a> {
    pub role: &'a str,
    pub kind: MemberKind,
    pub mode: Mode,
    pub check: Option<&'a str>,
    pub owns: &'a [&'a str],
    pub base_sha: Option<String>,
    pub cwd: PathBuf,
}

/// A team store, the fakes, and everything a `Ctx` borrows.
#[derive(Debug)]
pub struct Fixture {
    pub home: TestHome,
    pub store: Store,
    pub platform: TestPlatform,
    pub clock: FakeClock,
    pub registry: Registry,
    pub config: Config,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    pub const TEAM: &'static str = "alpha";

    pub fn new() -> Self {
        let home = TestHome::new().unwrap();
        let team = TeamName::parse(Self::TEAM).unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        let clock = FakeClock::new();
        store
            .write(|tx| {
                tx.create_team(
                    teamwire_core::Clock::now(&clock),
                    &NewTeam {
                        name: team.clone(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: None,
                    },
                )?;
                Ok(())
            })
            .unwrap();
        Self {
            home,
            store,
            platform: TestPlatform::new(),
            clock,
            registry: Registry::new(),
            config: Config::default(),
        }
    }

    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        }
    }

    /// `$TEAMWIRE_HOME/teams/alpha`.
    pub fn team_dir(&self) -> PathBuf {
        self.home.path().join("teams").join(Self::TEAM)
    }

    /// Adds a member and starts it. Returns the stored row.
    pub fn add_member(&mut self, setup: &MemberSetup<'_>) -> Member {
        let role = Role::parse(setup.role).unwrap();
        let now = teamwire_core::Clock::now(&self.clock);
        let new = NewMember {
            role: role.clone(),
            title: format!("the {}", setup.role),
            kind: setup.kind,
            mode: setup.mode,
            harness: HarnessId::from_static("claude-code"),
            host: HostId::from_static("herdr"),
            cwd: setup.cwd.clone(),
            repo: Some(setup.cwd.clone()),
            branch: Some("work".to_owned()),
            base: Some("main".to_owned()),
            base_sha: setup.base_sha.clone(),
            check: setup.check.map(str::to_owned),
            owns: setup.owns.iter().map(|o| (*o).to_owned()).collect(),
            deadline_s: 1800,
        };
        self.store
            .write(|tx| {
                tx.insert_member(now, &new)?;
                tx.set_member_state(now, &role, MemberState::Started, &json!({}))
            })
            .unwrap();
        self.member(setup.role)
    }

    pub fn member(&self, role: &str) -> Member {
        let role = Role::parse(role).unwrap();
        self.store.read(|tx| tx.member(&role)).unwrap().unwrap()
    }

    /// Writes `reports/<role>.md` under the team directory.
    pub fn write_report(&self, role: &str, text: &str) {
        let dir = self.team_dir().join("reports");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{role}.md")), text).unwrap();
    }
}

/// An implementer in a worktree of `repo`, started from its first commit.
pub fn implementer<'a>(
    role: &'a str,
    repo: &Repo,
    check: Option<&'a str>,
    owns: &'a [&'a str],
) -> MemberSetup<'a> {
    MemberSetup {
        role,
        kind: MemberKind::Implementer,
        mode: Mode::Worktree,
        check,
        owns,
        base_sha: Some(repo.git(&["rev-list", "--max-parents=0", "HEAD"])),
        cwd: repo.path().to_path_buf(),
    }
}

/// `FakeClock`'s starting time, in unix seconds.
pub fn start_secs() -> i64 {
    FakeClock::DEFAULT_START_MILLIS / 1000
}
