//! Helpers shared by the launch tests: a platform that really runs git and
//! `sh` (everything else is the in-process fake), a throwaway git
//! repository, and a rig that owns the fake adapters and a temporary home.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{
    Clock, Command, Config, Harness, HoldInfo, Host, Pid, Platform, PlatformError, Registry,
    RunOutput, SleepHold, TeamName,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_launch::{LaunchReport, SpecError, ValidPlan, ValidateEnv, parse_spec, validate};
use teamwire_store::{Ctx, Store};

/// Runs real processes for `run`; every other call goes to the fake.
#[derive(Debug, Default)]
pub(crate) struct GitPlatform {
    pub(crate) inner: FakePlatform,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the launch tests need real git and sh; this is the one place a test starts a process"
)]
fn real_run(cmd: &Command) -> Result<RunOutput, PlatformError> {
    let mut c = std::process::Command::new(&cmd.program);
    c.args(&cmd.args);
    if let Some(dir) = &cmd.cwd {
        c.current_dir(dir);
    }
    if cmd.env_clear {
        c.env_clear();
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

impl Platform for GitPlatform {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(&self, cmd: &Command, _timeout: Duration) -> Result<RunOutput, PlatformError> {
        real_run(cmd)
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

#[expect(
    clippy::disallowed_methods,
    reason = "a test makes its own throwaway git repository"
)]
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test writes its own fixture files"
)]
pub(crate) fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

/// A repository with one commit on `main` that ignores `.worktrees/`.
pub(crate) fn make_repo(parent: &Path, name: &str) -> PathBuf {
    let repo = parent.join(name);
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    write(&repo.join(".gitignore"), ".worktrees/\n");
    write(&repo.join("README.md"), "hello\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    repo
}

/// The worktree lock file launch and close take for the repository whose
/// git common directory (`<repo>/.git` for a main checkout) is `common`.
pub(crate) fn worktree_lock(home: &Path, common: &Path) -> PathBuf {
    use sha2::Digest as _;
    let key = std::fs::canonicalize(common).unwrap();
    let digest = sha2::Sha256::digest(key.to_string_lossy().as_bytes());
    home.join("locks")
        .join(format!("worktree-{}.lock", hex::encode(digest)))
}

#[derive(Debug)]
pub(crate) struct Rig {
    pub(crate) home: TestHome,
    pub(crate) work: tempfile::TempDir,
    pub(crate) platform: Arc<GitPlatform>,
    pub(crate) host: Arc<FakeHost>,
    pub(crate) harness: Arc<FakeHarness>,
    pub(crate) registry: Registry,
    pub(crate) clock: FakeClock,
    pub(crate) config: Config,
    pub(crate) user_home: PathBuf,
}

impl Rig {
    pub(crate) fn new() -> Self {
        Self::with_platform(GitPlatform::default())
    }

    /// A rig on a platform that has no sleep hold (Linux).
    pub(crate) fn without_hold() -> Self {
        Self::with_platform(GitPlatform {
            inner: FakePlatform::without_sleep_hold(),
        })
    }

    fn with_platform(platform: GitPlatform) -> Self {
        let host = Arc::new(FakeHost::new());
        let harness = Arc::new(FakeHarness::new());
        let mut registry = Registry::new();
        registry
            .add_harness(Arc::clone(&harness) as Arc<dyn Harness>)
            .unwrap();
        registry
            .add_host(Arc::clone(&host) as Arc<dyn Host>)
            .unwrap();
        harness.set_interactive_caller(teamwire_core::SessionId::parse("lead-session").unwrap());
        let work = tempfile::tempdir().unwrap();
        let user_home = work.path().join("userhome");
        std::fs::create_dir_all(&user_home).unwrap();
        Self {
            home: TestHome::new().unwrap(),
            work,
            platform: Arc::new(platform),
            host,
            harness,
            registry,
            clock: FakeClock::new(),
            config: Config::default(),
            user_home,
        }
    }

    pub(crate) fn repo(&self, name: &str) -> PathBuf {
        make_repo(self.work.path(), name)
    }

    pub(crate) fn env(&self) -> ValidateEnv<'_> {
        ValidateEnv {
            home: self.home.path(),
            registry: &self.registry,
            platform: &*self.platform,
            clock: &self.clock,
            config: &self.config,
            lead_harness: teamwire_core::HarnessId::from_static(FakeHarness::ID),
            lead_host: teamwire_core::HostId::from_static(FakeHost::ID),
            user_home: Some(self.user_home.clone()),
            teamwire_bin: PathBuf::from("/usr/local/bin/teamwire"),
        }
    }

    pub(crate) fn validate(&self, spec: &Value, add: bool) -> Result<ValidPlan, Vec<SpecError>> {
        let spec = parse_spec(&spec.to_string())?;
        validate(&spec, add, &self.env())
    }

    /// Opens the store (creating it) and launches.
    pub(crate) fn launch(&self, team: &str, plan: ValidPlan) -> (LaunchReport, Store) {
        let mut store = Store::open(self.home.path(), &TeamName::parse(team).unwrap()).unwrap();
        let report = {
            let mut ctx = Ctx {
                home: self.home.path(),
                store: &mut store,
                registry: &self.registry,
                platform: &*self.platform,
                clock: &self.clock as &dyn Clock,
                config: &self.config,
            };
            teamwire_launch::launch(&mut ctx, plan)
        };
        (report, store)
    }

    /// The home directory holds nothing: no team directory, no database.
    pub(crate) fn home_is_empty(&self) -> bool {
        std::fs::read_dir(self.home.path())
            .unwrap()
            .next()
            .is_none()
    }
}

/// A pane member that investigates in `cwd`.
pub(crate) fn pane_member(role: &str, title: &str, cwd: &Path) -> Value {
    json!({"role": role, "title": title, "mode": "pane", "cwd": cwd, "task": "look around"})
}

/// A worktree implementer in `repo`.
pub(crate) fn worktree_member(role: &str, title: &str, repo: &Path) -> Value {
    json!({"role": role, "title": title, "mode": "worktree", "repo": repo,
           "task": "build it", "check": "true", "owns": [format!("{role}/")]})
}

pub(crate) fn team_spec(team: &str, children: &[Value]) -> Value {
    json!({"team": team, "title": "Demo team", "children": children})
}

pub(crate) fn codes(errors: &[SpecError]) -> Vec<&str> {
    errors.iter().map(|e| e.code.as_str()).collect()
}

/// Keeps `HoldInfo` and `Pid` imports used by tests that include this file.
pub(crate) fn hold(pid: u32, owned: bool) -> HoldInfo {
    HoldInfo {
        pid: Pid::new(pid),
        owned,
    }
}

/// `value` with `key` set to `to`.
pub(crate) fn with(mut value: Value, key: &str, to: Value) -> Value {
    if let Some(map) = value.as_object_mut() {
        map.insert(key.to_owned(), to);
    }
    value
}

/// A team database that already holds a team row and the members made by
/// `setup`, as an earlier launch would have left it.
pub(crate) fn seed_team(
    rig: &Rig,
    team: &str,
    setup: impl FnOnce(
        &teamwire_store::Tx<'_>,
        teamwire_core::Timestamp,
    ) -> Result<(), teamwire_store::StoreError>,
) {
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(rig.home.path(), &name).unwrap();
    let at = rig.clock.now();
    store
        .write(|tx| {
            tx.create_team(
                at,
                &teamwire_store::NewTeam {
                    name: name.clone(),
                    title: "Seed".to_owned(),
                    lead_harness: teamwire_core::HarnessId::from_static(FakeHarness::ID),
                    lead_host: teamwire_core::HostId::from_static(FakeHost::ID),
                    lead_route: teamwire_core::LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            setup(tx, at)
        })
        .unwrap();
}

pub(crate) fn new_member(role: &str, title: &str, cwd: &Path) -> teamwire_store::NewMember {
    teamwire_store::NewMember {
        role: teamwire_core::Role::parse(role).unwrap(),
        title: title.to_owned(),
        kind: teamwire_core::MemberKind::Investigator,
        mode: teamwire_core::Mode::Pane,
        harness: teamwire_core::HarnessId::from_static(FakeHarness::ID),
        host: teamwire_core::HostId::from_static(FakeHost::ID),
        cwd: cwd.to_path_buf(),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: Vec::new(),
        deadline_s: 1800,
    }
}

/// The member row, read back from the store.
pub(crate) fn member_row(store: &Store, role: &str) -> Option<teamwire_core::Member> {
    let role = teamwire_core::Role::parse(role).unwrap();
    store.read(|tx| tx.member(&role)).unwrap()
}

/// The events written for one role, as (type, data).
pub(crate) fn role_events(store: &Store, role: &str) -> Vec<(String, Value)> {
    let role = teamwire_core::Role::parse(role).unwrap();
    store
        .read(|tx| tx.events_of_role(&role)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .map(|e| (e.kind.as_str().to_owned(), e.data))
        .collect()
}

/// A one-member team spec with a worktree implementer.
pub(crate) fn one_worktree_team(rig: &Rig, team: &str) -> (Value, PathBuf) {
    let repo = rig.repo("proj");
    (
        team_spec(team, &[worktree_member("api", "API work", &repo)]),
        repo,
    )
}

/// The value at a JSON pointer such as `/steps/setup`.
pub(crate) fn jget<'a>(value: &'a Value, pointer: &str) -> &'a Value {
    value.pointer(pointer).unwrap()
}

pub(crate) fn all_files_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all_files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A team database for another team of the same lead, with a recorded hold.
pub(crate) fn seed_lead_team(rig: &Rig, team: &str, hold: Option<HoldInfo>) {
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(rig.home.path(), &name).unwrap();
    let at = rig.clock.now();
    store
        .write(|tx| {
            tx.create_team(
                at,
                &teamwire_store::NewTeam {
                    name: name.clone(),
                    title: "Other".to_owned(),
                    lead_harness: teamwire_core::HarnessId::from_static(FakeHarness::ID),
                    lead_host: teamwire_core::HostId::from_static(FakeHost::ID),
                    lead_route: teamwire_core::LeadRoute::Sink,
                    lead_session: Some((
                        teamwire_core::SessionId::parse("lead-session").unwrap(),
                        teamwire_core::SessionKind::Interactive,
                    )),
                },
            )?;
            tx.set_sleep_hold(hold.as_ref())
        })
        .unwrap();
}

/// The hold recorded on the team row.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub(crate) fn team_hold(store: &Store) -> Option<HoldInfo> {
    store.read(|tx| tx.team()).unwrap().unwrap().sleep_hold
}

pub(crate) fn lead_at(rig: &Rig, pid: u32) {
    rig.harness.add_session(
        teamwire_core::SessionId::parse("lead-session").unwrap(),
        vec![Pid::new(pid)],
        teamwire_core::Liveness::Alive(teamwire_core::PlaceState::Working),
    );
}

pub(crate) fn launch_one(rig: &Rig, team: &str, role: &str, add: bool) -> (LaunchReport, Store) {
    let spec = team_spec(team, &[pane_member(role, role, rig.work.path())]);
    rig.launch(team, rig.validate(&spec, add).unwrap())
}
