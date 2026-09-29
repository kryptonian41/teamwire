//! One test's world: a temporary `TEAMWIRE_HOME`, a temporary git
//! repository, the fakes on `PATH`, and a lead that is a `fake-claude`
//! session in its own fake pane, so delivery to the lead goes through the
//! real locator (ADR 0006).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use teamwire_core::{Role, TeamName};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, TestHome};
use teamwire_store::Store;

use crate::bin::teamwire;
use crate::watch::Watch;

/// The lead's session id; `teamwire` commands run as the lead carry it in
/// `CLAUDE_CODE_SESSION_ID`, as a session of Claude Code would.
const LEAD_SESSION: &str = "lead-1";

/// What one run of `teamwire` printed.
#[derive(Debug, Clone)]
pub struct Run {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Run {
    /// Standard output and standard error, for an assertion message.
    pub fn all(&self) -> String {
        format!("exit {}\n{}{}", self.code, self.out, self.err)
    }
}

/// Polls `check` every 50 ms until it is true or `within` runs out.
pub fn wait_until(within: Duration, mut check: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + within;
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Debug)]
pub struct World {
    pub herdr: FakeHerdrEnv,
    pub home: TestHome,
    work: tempfile::TempDir,
    repo: PathBuf,
    lead_pane: String,
}

impl World {
    /// A world whose members run `member_script`, with a lead that does
    /// nothing but stop after each prompt.
    pub fn new(member_script: &ClaudeScript) -> Self {
        let mut herdr = FakeHerdrEnv::new().unwrap();
        herdr.set_script(member_script).unwrap();
        // Members run `teamwire ...` from their shell steps.
        let link = herdr.bin_dir().dir().join("teamwire");
        std::os::unix::fs::symlink(teamwire(), link).unwrap();
        let home = TestHome::new().unwrap();
        let work = tempfile::Builder::new()
            .prefix("teamwire-e2e-")
            .tempdir()
            .unwrap();
        let repo = work.path().join("proj");
        let mut world = Self {
            herdr,
            home,
            work,
            repo,
            lead_pane: String::new(),
        };
        world.make_repo();
        world.start_lead();
        world
    }

    fn make_repo(&self) {
        std::fs::create_dir_all(&self.repo).unwrap();
        self.git(&self.repo, &["init", "-q", "-b", "main"]);
        self.write(&self.repo.join(".gitignore"), ".worktrees/\n");
        self.git(&self.repo, &["add", "-A"]);
        self.git(&self.repo, &["commit", "-q", "-m", "first"]);
    }

    fn start_lead(&mut self) {
        let ws = self
            .herdr
            .run(&[
                "workspace",
                "create",
                "--cwd",
                self.work.path().to_str().unwrap(),
                "--label",
                "lead",
                "--no-focus",
            ])
            .unwrap();
        let pane = ws
            .result()
            .and_then(|r| r.pointer("/root_pane/pane_id")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| panic!("no lead pane: {ws:?}"));
        self.lead_pane = pane;
        self.start_lead_session(LEAD_SESSION);
    }

    /// Starts a `fake-claude` session named `session` in the lead's pane.
    pub fn start_lead_session(&self, session: &str) {
        let script = self
            .herdr
            .write_file("lead-script.json", r#"{"turns": [[]]}"#)
            .unwrap();
        let settings = self
            .herdr
            .write_file("lead-settings.json", r#"{"env":{},"hooks":{}}"#)
            .unwrap();
        let out = self
            .herdr
            .run_with(
                &[
                    "agent",
                    "start",
                    "lead",
                    "--kind",
                    "claude",
                    "--pane",
                    &self.lead_pane,
                    "--timeout",
                    "20000",
                    "--",
                    "-n",
                    "lead",
                    "--session-id",
                    session,
                    "--settings",
                    settings.to_str().unwrap(),
                ],
                &[("FAKE_CLAUDE_SCRIPT", script.to_str().unwrap())],
            )
            .unwrap();
        assert!(out.code == Some(0), "the lead did not start: {out:?}");
    }

    pub fn lead_pane(&self) -> &str {
        &self.lead_pane
    }

    /// The temporary directory the world's files live in.
    pub fn work(&self) -> &Path {
        self.work.path()
    }

    /// The temporary git repository, on `main`, with `.worktrees/` ignored.
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// Writes a file (the workspace forbids `std::fs::write` outside a
    /// helper that says why).
    pub fn write(&self, path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    /// `git -C <dir> <args>`; panics on failure and returns trimmed output.
    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
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

    /// The command every `teamwire` process of the world starts as: a
    /// cleared environment with the fakes' `PATH`, the temporary home, and
    /// the lead's pane and session, so the locator finds the lead.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(teamwire());
        cmd.args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", &self.lead_pane)
            .env("CLAUDE_CODE_SESSION_ID", LEAD_SESSION)
            .current_dir(&self.repo);
        cmd
    }

    /// Runs `teamwire <args>` as the lead and waits, with `stdin` if given.
    pub fn teamwire(&self, args: &[&str], stdin: Option<&str>) -> Run {
        let mut cmd = self.command(args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        if let Some(text) = stdin {
            input.write_all(text.as_bytes()).unwrap();
        }
        drop(input);
        let out = child.wait_with_output().unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// `teamwire launch` with the spec on standard input.
    pub fn launch(&self, spec: &Value, extra: &[&str]) -> Run {
        let mut args = vec!["launch"];
        args.extend_from_slice(extra);
        self.teamwire(&args, Some(&spec.to_string()))
    }

    /// Starts `teamwire watch <team> --interval <s>` in the background.
    pub fn watch(&self, team: &str, interval_s: u32) -> Watch {
        let out = self.work.path().join(format!("watch-{team}.out"));
        let file = std::fs::File::create(&out).unwrap();
        let mut cmd = self.command(&["watch", team, "--interval", &interval_s.to_string()]);
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Watch { child, out }
    }

    /// Opens the team's store for reading.
    pub fn store(&self, team: &str) -> Store {
        Store::open_read_only(self.home.path(), &TeamName::parse(team).unwrap()).unwrap()
    }

    pub fn team_dir(&self, team: &str) -> PathBuf {
        self.home.path().join("teams").join(team)
    }

    /// The text on a member's screen.
    pub fn member_screen(&self, team: &str, role: &str) -> String {
        self.screen(&format!("tw_{team}_{role}"))
    }

    /// The text on the lead's screen.
    pub fn lead_screen(&self) -> String {
        self.screen("lead")
    }

    fn screen(&self, agent: &str) -> String {
        let out = self
            .herdr
            .run(&["agent", "read", agent, "--lines", "200"])
            .unwrap();
        out.result()
            .and_then(|r| r.pointer("/read/text")?.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// Waits until every named member is `started` in the store.
    pub fn wait_started(&self, team: &str, roles: &[&str]) {
        let ok = wait_until(Duration::from_secs(30), || {
            let store = self.store(team);
            roles.iter().all(|r| {
                let role = Role::parse(r).unwrap();
                store
                    .read(|tx| tx.member(&role))
                    .ok()
                    .flatten()
                    .is_some_and(|m| m.state == teamwire_core::MemberState::Started)
            })
        });
        assert!(ok, "members did not all start: {roles:?}");
    }

    /// The command lines of running processes that mention `path`, from
    /// `pgrep -f`. A member's session and hooks carry paths under the home.
    pub fn processes_mentioning(&self, path: &Path) -> Vec<String> {
        let out = Command::new("pgrep").arg("-fl").arg(path).output().unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
