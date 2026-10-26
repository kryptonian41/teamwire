//! The shared kill harness: a team in a temporary home, live fake members in
//! fake herdr panes, the real `teamwire` binary run as a child process, and a
//! `SIGKILL` at a random moment. Randomness comes from one seed that is
//! printed, so a failing run replays with `TEAMWIRE_CRASH_SEED=<seed>`.
#![expect(
    clippy::disallowed_methods,
    reason = "the crash tests start the real teamwire binary and kill it"
)]
#![expect(
    clippy::print_stderr,
    reason = "the seed and the kill window are printed so a failing run can be replayed"
)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "helpers in an integration-test support file unwrap; a failure there fails the test"
)]

use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;
use teamwire_core::{
    Actor, HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode, Place, Recipient, Role,
    SessionId, SessionKind, TeamName, Timestamp,
};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, TestHome};
use teamwire_store::{NewMember, NewTeam, Store};

pub const TEAM: &str = "alpha";
pub const LEAD_SESSION: &str = "lead-1";

pub fn at() -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000).unwrap()
}

pub fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

/// SHA-256 of `text`, by the system `shasum`: this crate lists no hash
/// library, and the product hashes tokens with plain SHA-256.
pub fn sha256(text: &str) -> [u8; 32] {
    use std::io::Write as _;
    let mut child = Command::new("shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let hex = String::from_utf8(out.stdout).unwrap();
    let hex = hex.split_whitespace().next().unwrap();
    let mut bytes = [0_u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    bytes
}

/// The path of the `teamwire` binary: built once per test process, or with
/// `TEAMWIRE_FAKES_PREBUILT=1` the one already built, checked to be no older
/// than its sources (`teamwire_fakes::teamwire_cli`).
pub fn teamwire_bin() -> &'static Path {
    let bin = teamwire_fakes::teamwire_cli();
    assert!(
        bin.is_ok(),
        "cannot get the teamwire binary: {}",
        bin.clone().unwrap_err()
    );
    bin.unwrap()
}

/// A small, replayable random source (`SplitMix64`) seeded from `getrandom`,
/// or from `TEAMWIRE_CRASH_SEED` when a failure is replayed.
#[derive(Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn from_env(name: &str) -> Self {
        let seed = if let Ok(s) = std::env::var("TEAMWIRE_CRASH_SEED") {
            s.parse().expect("TEAMWIRE_CRASH_SEED is a number")
        } else {
            let mut b = [0_u8; 8];
            getrandom::fill(&mut b).unwrap();
            u64::from_le_bytes(b)
        };
        eprintln!("{name}: seed {seed} (replay with TEAMWIRE_CRASH_SEED={seed})");
        Self(seed)
    }

    /// The seed, for a failure message that says how to replay the run.
    pub fn seed(&self) -> u64 {
        self.0
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A duration in `[0, max]`, with microsecond steps.
    pub fn up_to(&mut self, max: Duration) -> Duration {
        let micros = u64::try_from(max.as_micros()).unwrap().max(1);
        Duration::from_micros(self.next_u64() % (micros + 1))
    }
}

#[derive(Debug)]
pub enum Identity {
    /// No identity variables: `launch` and `close` are run by the lead's shell.
    Nobody,
    Lead,
    Member(String),
}

/// A child process with its stderr and stdout going to files, so output
/// written before a kill can be read after it, even when a grandchild (a
/// fake herdr call) still holds a pipe open.
const CHILD_LIMIT: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct Running {
    pub child: Child,
    out: std::fs::File,
    err: std::fs::File,
}

#[derive(Debug)]
pub struct Ended {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Running {
    pub fn kill9(&mut self) {
        // The child may already have exited; that is a legal kill point.
        kill(
            Pid::from_raw(self.child.id().try_into().unwrap()),
            Signal::SIGKILL,
        )
        .ok();
    }

    pub fn finish(mut self) -> Ended {
        // Plain `cargo test` has no time limit of its own, so a child that
        // hangs would hang the whole run.
        let begun = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            let hung = begun.elapsed() > CHILD_LIMIT;
            if hung {
                // The whole group: a hung grandchild must not outlive the test.
                killpg(
                    Pid::from_raw(self.child.id().try_into().unwrap()),
                    Signal::SIGKILL,
                )
                .ok();
                self.child.wait().unwrap();
            }
            assert!(
                !hung,
                "a teamwire child did not finish within {CHILD_LIMIT:?}: {}",
                slurp(&mut self.err)
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        Ended {
            code: status.code(),
            stdout: slurp(&mut self.out),
            stderr: slurp(&mut self.err),
        }
    }

    /// What the child has written to stderr so far.
    pub fn stderr_so_far(&mut self) -> String {
        slurp(&mut self.err)
    }
}

fn slurp(file: &mut std::fs::File) -> String {
    let mut text = String::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_string(&mut text).unwrap();
    text
}

#[derive(Debug)]
pub struct Rig {
    pub herdr: FakeHerdrEnv,
    pub home: TestHome,
    /// A directory put before the fakes on the `PATH` of every command, for a
    /// wrapper that slows one host call down.
    pub path_prefix: Option<PathBuf>,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    /// A home with no team in it: `launch` makes the team.
    pub fn bare() -> Self {
        let mut herdr = FakeHerdrEnv::new().unwrap();
        herdr.set_script(&ClaudeScript::default()).unwrap();
        let home = TestHome::new().unwrap();
        Self {
            herdr,
            home,
            path_prefix: None,
        }
    }

    /// Team `alpha`, its lead session `lead-1`, no members yet.
    pub fn new() -> Self {
        let rig = Self::bare();
        let mut store = Store::open(rig.home.path(), &team_name()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at(),
                    &NewTeam {
                        name: team_name(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse(LEAD_SESSION).unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
        rig
    }

    pub fn store(&self) -> Store {
        Store::open(self.home.path(), &team_name()).unwrap()
    }

    /// Starts a `fake-claude` session named `name` with session id `session`
    /// in a new herdr pane, and returns the pane.
    pub fn start_agent(&self, name: &str, session: &str) -> Place {
        let ws = self
            .herdr
            .run(&[
                "workspace",
                "create",
                "--cwd",
                "/tmp",
                "--label",
                name,
                "--no-focus",
            ])
            .unwrap();
        let pane = ws
            .result()
            .and_then(|r| r.pointer("/root_pane/pane_id")?.as_str().map(str::to_owned))
            .unwrap();
        let settings = self
            .herdr
            .write_file(&format!("{name}-settings.json"), r#"{"env":{},"hooks":{}}"#)
            .unwrap();
        let started = self
            .herdr
            .run(&[
                "agent",
                "start",
                name,
                "--kind",
                "claude",
                "--pane",
                &pane,
                "--timeout",
                "20000",
                "--",
                "-n",
                name,
                "--session-id",
                session,
                "--settings",
                &settings.display().to_string(),
            ])
            .unwrap();
        assert_eq!(started.code, Some(0), "{started:?}");
        Place {
            host: HostId::from_static("herdr"),
            id: pane,
        }
    }

    /// The lead's live session, so a result can be typed into it.
    pub fn start_lead(&self) {
        self.start_agent("lead-agent", LEAD_SESSION);
    }

    /// A member of team alpha. `live` starts a `fake-claude` session in its
    /// own herdr pane; otherwise the member is recorded and no process stands
    /// behind it.
    pub fn add_member(&self, role: &str, kind: MemberKind, live: bool) {
        self.add_member_to(TEAM, role, kind, live);
    }

    /// Another team in the same home, led from session `lead-<name>`, with no
    /// members yet.
    pub fn add_team(&self, name: &str) {
        let team = TeamName::parse(name).unwrap();
        let mut store = Store::open(self.home.path(), &team).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at(),
                    &NewTeam {
                        name: team.clone(),
                        title: name.to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse(&format!("lead-{name}")).unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    }

    /// Team `team`'s store.
    pub fn store_of(&self, team: &str) -> Store {
        Store::open(self.home.path(), &TeamName::parse(team).unwrap()).unwrap()
    }

    /// `add_member` for any team. A session id is unique across teams.
    pub fn add_member_to(&self, team: &str, role: &str, kind: MemberKind, live: bool) {
        let session = if team == TEAM {
            format!("session-{role}")
        } else {
            format!("session-{team}-{role}")
        };
        let place = live.then(|| self.start_agent(&format!("tw_{team}_{role}"), &session));
        let r = Role::parse(role).unwrap();
        let hash = sha256(&token_of(role));
        let mut store = self.store_of(team);
        store
            .write(|tx| {
                tx.insert_member(
                    at(),
                    &NewMember {
                        role: r.clone(),
                        title: role.to_owned(),
                        kind,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/tmp".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_session(&r, &SessionId::parse(&session).unwrap())?;
                if let Some(place) = &place {
                    tx.set_member_place(&r, place)?;
                }
                tx.set_member_token_hash(&r, &hash)?;
                tx.set_member_state(at(), &r, MemberState::Started, &serde_json::json!({}))
            })
            .unwrap();
    }

    /// `teamwire <args>` with a cleared environment: the fakes' `PATH`, the
    /// temporary home, and the caller's identity.
    pub fn command(&self, args: &[&str], identity: &Identity) -> Command {
        let mut cmd = Command::new(teamwire_bin());
        // Its own process group, so a hung run can be killed with its children.
        cmd.args(args)
            .process_group(0)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .stdin(Stdio::null());
        if let Some(prefix) = &self.path_prefix {
            let path = self
                .herdr
                .env()
                .into_iter()
                .find_map(|(k, v)| (k == "PATH").then_some(v))
                .unwrap();
            cmd.env("PATH", format!("{}:{path}", prefix.display()));
        }
        match identity {
            Identity::Member(role) => {
                cmd.env("TEAMWIRE_TEAM", TEAM)
                    .env("TEAMWIRE_ROLE", role)
                    .env("TEAMWIRE_TOKEN", token_of(role));
            }
            Identity::Nobody => {}
            Identity::Lead => {
                cmd.env("CLAUDE_CODE_SESSION_ID", LEAD_SESSION);
            }
        }
        cmd
    }

    /// Starts the command with output going to files.
    pub fn start(&self, args: &[&str], identity: &Identity) -> Running {
        let out = tempfile::tempfile().unwrap();
        let err = tempfile::tempfile().unwrap();
        let mut cmd = self.command(args, identity);
        cmd.stdout(Stdio::from(out.try_clone().unwrap()))
            .stderr(Stdio::from(err.try_clone().unwrap()));
        Running {
            child: cmd.spawn().unwrap(),
            out,
            err,
        }
    }

    /// Like `start`, with `stdin` as the child's standard input.
    pub fn start_with_stdin(&self, args: &[&str], identity: &Identity, stdin: &str) -> Running {
        use std::io::Write as _;
        let mut input = tempfile::tempfile().unwrap();
        input.write_all(stdin.as_bytes()).unwrap();
        input.seek(SeekFrom::Start(0)).unwrap();
        let out = tempfile::tempfile().unwrap();
        let err = tempfile::tempfile().unwrap();
        let mut cmd = self.command(args, identity);
        cmd.stdin(Stdio::from(input))
            .stdout(Stdio::from(out.try_clone().unwrap()))
            .stderr(Stdio::from(err.try_clone().unwrap()));
        Running {
            child: cmd.spawn().unwrap(),
            out,
            err,
        }
    }

    /// From now on, a `herdr <words>` call runs the fake, then creates the
    /// returned signal file and waits six seconds before it returns. A caller
    /// stuck there is between the host call and whatever it records next.
    pub fn pause_after(&mut self, words: &str) -> PathBuf {
        let test = format!("[ \"$1 $2\" = '{words}' ]");
        self.pause_wrapper("herdr", "fake-herdr", &test)
    }

    /// Like `pause_after` for git: a `git -C <dir> <words>` call runs the
    /// real git, then pauses. The real git is named by its absolute path,
    /// because the wrapper stands first on the `PATH`.
    pub fn pause_after_git(&mut self, words: &str) -> PathBuf {
        let real = real_git();
        let test = format!("case \" $* \" in *' {words} '*) true;; *) false;; esac");
        self.pause_wrapper("git", &real.display().to_string(), &test)
    }

    /// Writes `slow/<tool>`: run `real`, and when `test` holds for the
    /// arguments, create the signal file and sleep six seconds.
    fn pause_wrapper(&mut self, tool: &str, real: &str, test: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let signal = self.herdr.root().join(format!("paused-{tool}"));
        let script = format!(
            "#!/bin/sh\n{real} \"$@\"\nrc=$?\nif {test}; then\n  : > '{}'\n  sleep 6\nfi\nexit $rc\n",
            signal.display()
        );
        std::fs::create_dir_all(self.herdr.root().join("slow")).unwrap();
        let path = self
            .herdr
            .write_file(&format!("slow/{tool}"), &script)
            .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        self.path_prefix = Some(path.parent().unwrap().to_path_buf());
        signal
    }

    /// Waits until `signal` exists, failing after 20 seconds.
    pub fn wait_for(signal: &Path, running: &mut Running) {
        let waited = Instant::now();
        while !signal.exists() {
            assert!(
                waited.elapsed() < Duration::from_secs(20),
                "the pause point was never reached: {}",
                running.stderr_so_far()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// How many times a text was typed into a pane by `agent prompt`.
    pub fn typed_count(&self, text: &str) -> usize {
        self.herdr
            .calls()
            .iter()
            .filter(|c| c.command() == "agent prompt" && c.argv.iter().any(|a| a.contains(text)))
            .count()
    }

    pub fn run(&self, args: &[&str], identity: &Identity) -> Ended {
        self.start(args, identity).finish()
    }

    /// Starts the command, waits `delay`, sends `SIGKILL`, and returns what
    /// the child had written by then.
    pub fn run_and_kill(&self, args: &[&str], identity: &Identity, delay: Duration) -> Ended {
        let mut running = self.start(args, identity);
        let started = Instant::now();
        while started.elapsed() < delay {
            std::hint::spin_loop();
        }
        running.kill9();
        running.finish()
    }
}

pub fn team_name() -> TeamName {
    TeamName::parse(TEAM).unwrap()
}

/// The store's own consistency check.
pub fn assert_integrity(store: &Store) {
    assert_eq!(store.integrity_check().unwrap(), "ok");
}

/// Every stored message, oldest first. A damaged row fails the test.
pub fn all_messages(store: &Store) -> Vec<teamwire_core::Message> {
    let all = store
        .read(|tx| tx.messages(&teamwire_store::MessageFilter::default()))
        .unwrap();
    assert!(all.bad.is_empty(), "{:?}", all.bad);
    all.items
}

/// How many stored messages carry this body. The tests send each message with
/// its own key as the body, so a count above 1 is a duplicate row.
pub fn count_with_body(store: &Store, body: &str) -> usize {
    all_messages(store)
        .iter()
        .filter(|m| m.body == body)
        .count()
}

pub fn recipient(role: &str) -> Recipient {
    Recipient::parse(role).unwrap()
}

pub fn actor(name: &str) -> Actor {
    Actor::parse(name).unwrap()
}

/// Runs a launch of one pane member `scout` in team `alpha`, and asserts it
/// started.
pub fn launch_scout(rig: &Rig, work: &Path) {
    let spec = serde_json::json!({"team": "alpha", "title": "Alpha", "children": [
        {"role": "scout", "title": "Scout", "mode": "pane", "cwd": work, "task": "look around"}]});
    let out = rig
        .start_with_stdin(&["launch"], &Identity::Nobody, &spec.to_string())
        .finish();
    assert_eq!(out.code, Some(0), "{out:?}");
    wait_idle(rig);
}

/// Waits until `close --dry-run` no longer calls a member busy: `close`
/// refuses a busy member, and a fresh member works on its first task for a
/// moment.
pub fn wait_idle(rig: &Rig) {
    let waited = Instant::now();
    loop {
        let out = rig.run(&["close", "alpha", "--dry-run"], &Identity::Nobody);
        if !out.stdout.contains("busy") && !out.stderr.contains("busy") {
            return;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "the member stayed busy: {out:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The first `git` on the test `PATH`, by absolute path.
fn real_git() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap();
    std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|p| p.is_file())
        .expect("no git on the PATH")
}

/// Runs `git -C dir args` with a fixed identity and asserts it succeeded.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(real_git())
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

/// Writes `name` into the checkout `dir` and commits it.
pub fn commit_file(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", name]);
}

/// A repository on `main` with one commit, ignoring `.worktrees/` as launch
/// requires.
pub fn temp_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(repo.path().join(".gitignore"), ".worktrees/\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "first"]);
    repo
}

/// The launch spec of one worktree member `wt` of team `alpha` in `repo`.
pub fn worktree_spec(repo: &Path) -> String {
    serde_json::json!({"team": "alpha", "title": "Alpha", "children": [
        {"role": "wt", "title": "Worktree", "kind": "implementer", "mode": "worktree",
         "repo": repo, "task": "build", "check": "true", "owns": ["a.txt"]}]})
    .to_string()
}

/// Every archive directory of the home, oldest name first.
pub fn archives(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(home.join("archive")) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
    dirs.sort();
    dirs
}

/// Runs `close` until it succeeds, clearing a lock a dead close left. Returns
/// how many runs it took.
pub fn close_until_done(rig: &Rig) -> usize {
    let mut finished = None;
    for run in 1..=4 {
        let out = rig.run(&["close", "alpha"], &Identity::Nobody);
        if out.code == Some(0) {
            finished = Some(run);
            break;
        }
        assert!(
            out.stderr.contains("--clear-stale-lock"),
            "close refused for another reason: {out:?}"
        );
        let cleared = rig.run(&["close", "alpha", "--clear-stale-lock"], &Identity::Nobody);
        assert_eq!(cleared.code, Some(0), "{cleared:?}");
    }
    finished.expect("close never finished")
}

/// The state directory is gone and the newest archive matches its sums.
pub fn assert_closed_for_good(rig: &Rig) {
    assert!(
        !rig.home.path().join("teams/alpha").exists(),
        "the state directory is still there"
    );
    let all = archives(rig.home.path());
    let newest = all.last().expect("an archive exists");
    assert!(
        archive_verifies(newest),
        "the newest archive {} does not match its SHA256SUMS",
        newest.display()
    );
}

/// True when every file of the archive hashes to its `SHA256SUMS` line.
pub fn archive_verifies(dir: &Path) -> bool {
    Command::new("shasum")
        .args(["-a", "256", "-c", "SHA256SUMS"])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The Stop-hook input of member `api`, with a hold already active so the
/// hook goes straight to delivery.
pub const STOP: &str =
    r#"{"session_id":"session-api","hook_event_name":"Stop","stop_hook_active":true}"#;

pub fn hook_args() -> Vec<&'static str> {
    vec!["hook", "claude-stop", "--team", "alpha", "--role", "api"]
}

/// Runs the Stop hook of member `api` to the end: "one more turn end".
pub fn stop_hook(rig: &Rig) -> Ended {
    let me = Identity::Member("api".to_owned());
    let out = rig.start_with_stdin(&hook_args(), &me, STOP).finish();
    assert_eq!(out.code, Some(0), "{out:?}");
    out
}

/// Stops every agent the fake host still lists, then asserts none is left.
pub fn stop_agents(rig: &Rig) {
    let list = rig.herdr.run(&["agent", "list"]).unwrap();
    let panes: Vec<String> = list
        .result()
        .and_then(|r| r.get("agents")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|a| a.get("pane_id")?.as_str().map(str::to_owned))
        .collect();
    for pane in &panes {
        rig.herdr.run(&["pane", "close", pane]).unwrap();
    }
    let after = rig.herdr.run(&["agent", "list"]).unwrap();
    let left = after
        .result()
        .and_then(|r| r.get("agents")?.as_array().map(Vec::len))
        .unwrap_or(0);
    assert_eq!(left, 0, "agents are still running: {after:?}");
}
