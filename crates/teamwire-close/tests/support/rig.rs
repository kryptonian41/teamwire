//! Shared by the integration tests: a platform that really runs git (every
//! other call goes to the in-process fake), a real repository with real
//! worktrees, the fake host and harness, and a team store in a temporary home.
#![expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "test support: real git worktrees need real processes, and a failure here fails the test"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Clock, Command, Config, HarnessId, HoldInfo, HostId, LeadRoute, Liveness, Member, MemberKind,
    MemberState, Mode, Occupant, Pid, Place, PlaceState, Platform, PlatformError, Registry, Role,
    RunOutput, SessionId, SessionKind, SleepHold, TeamName,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_store::{Ctx, NewMember, NewTeam, Store, TeamRow};

pub const TEAM: &str = "alpha";
pub const LEAD_PID: u32 = 4242;

/// Runs real processes for `run`; every other call goes to the fake.
#[derive(Debug, Default)]
pub struct GitPlatform {
    pub inner: FakePlatform,
}

impl Platform for GitPlatform {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn process_started(&self, pid: Pid) -> Option<teamwire_core::Timestamp> {
        self.inner.process_started(pid)
    }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(&self, cmd: &Command, _timeout: Duration) -> Result<RunOutput, PlatformError> {
        let mut c = std::process::Command::new(&cmd.program);
        c.args(&cmd.args);
        if let Some(dir) = &cmd.cwd {
            c.current_dir(dir);
        }
        c.envs(cmd.env.iter().map(|(k, v)| (k, v)));
        c.stdin(std::process::Stdio::null());
        let out = c
            .output()
            .map_err(|e| PlatformError::new(teamwire_core::PlatformErrorCode::Io, e.to_string()))?;
        Ok(RunOutput {
            exit_code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
            timed_out: false,
        })
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

/// Runs git in `dir`; panics unless it exits 0. Returns trimmed stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// A team store, the fakes, a real repository, and everything a `Ctx` borrows.
#[derive(Debug)]
pub struct Rig {
    pub home: TestHome,
    pub store: Store,
    pub platform: Arc<GitPlatform>,
    pub clock: FakeClock,
    pub registry: Registry,
    pub config: Config,
    pub host: Arc<FakeHost>,
    pub harness: Arc<FakeHarness>,
    pub work: tempfile::TempDir,
    pub repo: PathBuf,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    pub fn new() -> Self {
        Self::build(true)
    }

    /// A rig whose `repo` path does not exist, for tests that only use panes.
    /// Starting git there fails, and `add_worktree_member` refuses to run.
    pub fn without_repo() -> Self {
        Self::build(false)
    }

    fn build(with_repo: bool) -> Self {
        let home = TestHome::new().unwrap();
        let work = tempfile::tempdir().unwrap();
        let repo = work.path().join("proj");
        if with_repo {
            std::fs::create_dir_all(&repo).unwrap();
            git(&repo, &["init", "-q", "-b", "main"]);
            std::fs::write(repo.join("README.md"), "hello\n").unwrap();
            git(&repo, &["add", "-A"]);
            git(&repo, &["commit", "-q", "-m", "first"]);
        }
        let host = Arc::new(FakeHost::with_id(HostId::from_static("herdr")));
        let harness = Arc::new(FakeHarness::with_id(HarnessId::from_static("claude-code")));
        harness.add_session(
            SessionId::parse("lead-1").unwrap(),
            vec![Pid::new(LEAD_PID)],
            Liveness::Alive(PlaceState::Idle),
        );
        let mut registry = Registry::new();
        registry.add_host(host.clone()).unwrap();
        registry.add_harness(harness.clone()).unwrap();
        let team = TeamName::parse(TEAM).unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        let clock = FakeClock::new();
        store
            .write(|tx| {
                tx.create_team(
                    clock.now(),
                    &NewTeam {
                        name: team.clone(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse("lead-1").unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )?;
                Ok(())
            })
            .unwrap();
        Self {
            home,
            store,
            platform: Arc::new(GitPlatform::default()),
            clock,
            registry,
            config: Config::default(),
            host,
            harness,
            work,
            repo,
        }
    }

    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &*self.platform,
            clock: &self.clock,
            config: &self.config,
        }
    }

    /// `$TEAMWIRE_HOME/teams/alpha`.
    pub fn team_dir(&self) -> PathBuf {
        self.home.path().join("teams").join(TEAM)
    }

    /// `$TEAMWIRE_HOME/archive`.
    pub fn archive_root(&self) -> PathBuf {
        self.home.path().join("archive")
    }

    /// A file in the team directory, as the team would leave it.
    pub fn team_file(&self, rel: &str, text: &str) {
        let path = self.team_dir().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    pub fn team(&self) -> TeamRow {
        self.store.read(|tx| tx.team()).unwrap().unwrap()
    }

    pub fn member(&self, role: &str) -> Member {
        let role = Role::parse(role).unwrap();
        self.store.read(|tx| tx.member(&role)).unwrap().unwrap()
    }

    /// An implementer (or another kind) in a real worktree of the repository,
    /// on branch `br-<role>`, with an idle place in the fake host, started.
    pub fn add_worktree_member(&mut self, role: &str, kind: MemberKind) -> Member {
        assert!(
            self.repo.exists(),
            "this rig was built with Rig::without_repo(); use Rig::new() for worktree members"
        );
        let path = self.work.path().join(format!("wt-{role}"));
        let branch = format!("br-{role}");
        git(
            &self.repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                &path.display().to_string(),
            ],
        );
        let base_sha = git(&self.repo, &["rev-parse", "main"]);
        self.add_member(
            role,
            kind,
            Mode::Worktree,
            path,
            Some(branch),
            Some(base_sha),
        )
    }

    /// A pane-mode member: no worktree, an idle place, started.
    pub fn add_pane_member(&mut self, role: &str, kind: MemberKind) -> Member {
        let cwd = self.work.path().to_path_buf();
        self.add_member(role, kind, Mode::Pane, cwd, None, None)
    }

    fn add_member(
        &mut self,
        role: &str,
        kind: MemberKind,
        mode: Mode,
        cwd: PathBuf,
        branch: Option<String>,
        base_sha: Option<String>,
    ) -> Member {
        let parsed = Role::parse(role).unwrap();
        let place = self.host.add_place(
            &format!("tw_{TEAM}_{role}"),
            PlaceState::Idle,
            Occupant {
                session: None,
                pids: Vec::new(),
            },
        );
        let now = self.clock.now();
        let new = NewMember {
            role: parsed.clone(),
            title: format!("the {role}"),
            kind,
            mode,
            harness: HarnessId::from_static("claude-code"),
            host: HostId::from_static("herdr"),
            cwd,
            repo: (mode == Mode::Worktree).then(|| self.repo.clone()),
            branch,
            base: Some("main".to_owned()),
            base_sha,
            check: None,
            owns: Vec::new(),
            deadline_s: 1800,
        };
        self.store
            .write(|tx| {
                tx.insert_member(now, &new)?;
                tx.set_member_place(&parsed, &place)?;
                tx.set_member_state(now, &parsed, MemberState::Started, &json!({}))
            })
            .unwrap();
        self.member(role)
    }

    pub fn place_of(&self, role: &str) -> Place {
        self.member(role).place.unwrap()
    }

    pub fn set_place_state(&self, role: &str, state: PlaceState) {
        self.host.set_state(&self.place_of(role), state);
    }

    /// Records a sleep hold on the team.
    pub fn record_hold(&mut self, hold: HoldInfo) {
        self.store
            .write(|tx| tx.set_sleep_hold(Some(&hold)))
            .unwrap();
    }

    /// Makes the fake platform's hold real, for the lead's process.
    pub fn start_owned_hold(&mut self) -> HoldInfo {
        let started = self
            .platform
            .inner
            .sleep_hold()
            .unwrap()
            .start(Pid::new(LEAD_PID))
            .unwrap();
        self.record_hold(started);
        started
    }

    /// Every event of `kind`, in order.
    pub fn events(&self, kind: teamwire_core::EventType) -> Vec<teamwire_core::Event> {
        self.store
            .read(|tx| tx.events_of_type(kind)?.all_or_corrupt())
            .unwrap()
    }
}

/// Every entry of `dir`, sorted; empty when it does not exist.
pub fn entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut all: Vec<PathBuf> = read.map(|e| e.unwrap().path()).collect();
    all.sort();
    all
}
