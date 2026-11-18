//! Everything a test needs to drive `fake-herdr` and `fake-claude` as
//! processes: a private state directory, the environment to run them with,
//! fault and script builders, and a reader for the call log.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::bins::{FakeBinDir, FakeBins};
use crate::claude_bin::Script;
use crate::fsutil;
use crate::herdr_bin::{Action, ErrorBody, Rule};
use crate::owner;

/// Builds the fault file for `FAKE_HERDR_FAULTS`.
#[derive(Debug, Clone, Default)]
pub struct HerdrFaults {
    rules: Vec<Rule>,
}

impl HerdrFaults {
    pub fn new() -> Self {
        Self::default()
    }

    fn rule(mut self, cmd: &str, target: Option<&str>, times: Option<u32>, action: Action) -> Self {
        self.rules.push(Rule {
            cmd: Some(cmd.to_owned()),
            target: target.map(str::to_owned),
            times,
            action,
        });
        self
    }

    /// The command fails `times` times with this `error.code` (always if
    /// `times` is `None`), then works. Example: `("agent start", None, Some(2),
    /// "agent_pane_busy")` is busy twice, then success.
    #[must_use]
    pub fn error(self, cmd: &str, target: Option<&str>, times: Option<u32>, code: &str) -> Self {
        let body = ErrorBody {
            code: code.to_owned(),
            message: format!("fake-herdr injected {code}"),
        };
        self.rule(cmd, target, times, Action::Error(body))
    }

    /// The command hangs (see `FAKE_HERDR_HANG_MS`).
    #[must_use]
    pub fn hang(self, cmd: &str, target: Option<&str>, times: Option<u32>) -> Self {
        self.rule(cmd, target, times, Action::Hang)
    }

    /// The command prints text that is not JSON and exits 0.
    #[must_use]
    pub fn garbage(self, cmd: &str, target: Option<&str>, times: Option<u32>) -> Self {
        self.rule(cmd, target, times, Action::Garbage)
    }

    /// `agent prompt --wait` types the text, then fails with
    /// `agent_prompt_stalled`. Without `--wait` the call works, as in herdr.
    #[must_use]
    pub fn stalled_prompt(self, target: Option<&str>, times: Option<u32>) -> Self {
        self.rule("agent prompt", target, times, Action::Stalled)
    }

    /// `agent prompt` types the text, then fails with `timeout`, the answer
    /// of a call that ran out of time. The adapter passes no `--wait`, so
    /// this is the failure after typing that delivery can reach.
    #[must_use]
    pub fn timed_out_prompt(self, target: Option<&str>, times: Option<u32>) -> Self {
        self.rule("agent prompt", target, times, Action::TimedOut)
    }

    /// `agent_not_found` for every call of `cmd` on `target`.
    #[must_use]
    pub fn agent_not_found(self, cmd: &str, target: &str) -> Self {
        self.error(cmd, Some(target), None, "agent_not_found")
    }

    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        let doc = serde_json::json!({ "rules": self.rules });
        fsutil::write_atomic(path, doc.to_string().as_bytes())
    }
}

impl Script {
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        fsutil::write_atomic(path, &bytes)
    }
}

/// One line of the `FAKE_HERDR_LOG`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LoggedCall {
    pub argv: Vec<String>,
    /// `"read"` or `"write"`.
    pub kind: String,
    /// The injected fault that fired, such as `"error:agent_pane_busy"`.
    pub fault: Option<String>,
}

impl LoggedCall {
    /// The command words, such as `"agent get"`.
    pub fn command(&self) -> String {
        self.argv
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What one run of `fake-herdr` printed.
#[derive(Debug, Clone)]
pub struct CliOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CliOutput {
    /// The `result` object of a success, or `None`.
    pub fn result(&self) -> Option<Value> {
        let doc: Value = serde_json::from_str(self.stdout.trim()).ok()?;
        doc.get("result").cloned()
    }

    /// The `error.code` of a failure envelope on stderr, or `None`.
    pub fn error_code(&self) -> Option<String> {
        let doc: Value = serde_json::from_str(self.stderr.trim()).ok()?;
        doc.pointer("/error/code")?.as_str().map(str::to_owned)
    }
}

/// A private world for `fake-herdr` and `fake-claude`: state file, log,
/// fault file, script file, `TEAMWIRE_CLAUDE_DIR`, home, and a bin directory
/// on a `PATH` that cannot reach the real programs. Removed on drop.
///
/// The world holds the owner lock (see `owner.rs`) for its whole life, so
/// its agents end themselves when this process dies without dropping it.
#[derive(Debug)]
pub struct FakeHerdrEnv {
    root: tempfile::TempDir,
    bins: FakeBinDir,
    faults_set: bool,
    script_set: bool,
    _owner_lock: std::fs::File,
}

impl FakeHerdrEnv {
    pub fn new() -> Result<Self, String> {
        let root = tempfile::Builder::new()
            .prefix("fake-herdr-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        for sub in ["home", "claude", "tmp"] {
            std::fs::create_dir_all(root.path().join(sub)).map_err(|e| e.to_string())?;
        }
        let bins = FakeBins::ensure()?
            .install(&root.path().join("bin"))
            .map_err(|e| e.to_string())?;
        let owner_lock = owner::hold(&owner_lock_path(root.path())).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            bins,
            faults_set: false,
            script_set: false,
            _owner_lock: owner_lock,
        })
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn bin_dir(&self) -> &FakeBinDir {
        &self.bins
    }

    /// `TEAMWIRE_CLAUDE_DIR` for this world: `sessions/<pid>.json` files go here.
    pub fn claude_dir(&self) -> PathBuf {
        self.root.path().join("claude")
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.path().join("herdr-state.json")
    }

    pub fn log_path(&self) -> PathBuf {
        self.root.path().join("herdr-log.jsonl")
    }

    pub fn faults_path(&self) -> PathBuf {
        self.root.path().join("herdr-faults.json")
    }

    pub fn script_path(&self) -> PathBuf {
        self.root.path().join("claude-script.json")
    }

    pub fn set_faults(&mut self, faults: &HerdrFaults) -> std::io::Result<()> {
        faults.write_to(&self.faults_path())?;
        self.faults_set = true;
        Ok(())
    }

    pub fn set_script(&mut self, script: &Script) -> std::io::Result<()> {
        script.write_to(&self.script_path())?;
        self.script_set = true;
        Ok(())
    }

    /// The whole environment for a process that runs `herdr`: `PATH` with
    /// the fakes first, and the `FAKE_*` and `TEAMWIRE_CLAUDE_DIR` variables.
    /// Start the process with a cleared environment plus these.
    pub fn env(&self) -> Vec<(String, String)> {
        let text = |p: PathBuf| p.display().to_string();
        let mut env = vec![
            (
                "PATH".to_owned(),
                self.bins.path().to_string_lossy().into_owned(),
            ),
            ("HOME".to_owned(), text(self.root.path().join("home"))),
            ("TMPDIR".to_owned(), text(self.root.path().join("tmp"))),
            ("FAKE_HERDR_STATE".to_owned(), text(self.state_path())),
            ("FAKE_HERDR_LOG".to_owned(), text(self.log_path())),
            ("TEAMWIRE_CLAUDE_DIR".to_owned(), text(self.claude_dir())),
            ("FAKE_HERDR_HANG_MS".to_owned(), "20000".to_owned()),
            (
                owner::OWNER_LOCK_VAR.to_owned(),
                text(owner_lock_path(self.root.path())),
            ),
        ];
        if self.faults_set {
            env.push(("FAKE_HERDR_FAULTS".to_owned(), text(self.faults_path())));
        }
        if self.script_set {
            env.push(("FAKE_CLAUDE_SCRIPT".to_owned(), text(self.script_path())));
        }
        env
    }

    /// Runs `herdr <args>` (the fake) with `env()` and waits.
    pub fn run(&self, args: &[&str]) -> std::io::Result<CliOutput> {
        self.run_with(args, &[])
    }

    /// Like `run`, with `extra` variables on top of `env()`. A variable in
    /// `extra` that `fake-herdr` must not pass on (`TEAMWIRE_TOKEN`) tests
    /// the cleared environment.
    pub fn run_with(&self, args: &[&str], extra: &[(&str, &str)]) -> std::io::Result<CliOutput> {
        self.run_program("herdr", args, extra)
    }

    /// Runs `claude <args>` (the fake) with `env()`: for `claude agents --json`.
    pub fn run_claude(&self, args: &[&str]) -> std::io::Result<CliOutput> {
        self.run_program("claude", args, &[])
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "tests drive the fake programs as processes; this crate may start processes"
    )]
    fn run_program(
        &self,
        program: &str,
        args: &[&str],
        extra: &[(&str, &str)],
    ) -> std::io::Result<CliOutput> {
        // The output goes to files, and the call ends when the program exits.
        // With pipes, the call would end only when every holder of the pipe
        // closes it. On macOS the standard library makes a pipe and marks it
        // close-on-exec in two steps, so a process another thread starts in
        // between inherits the pipe, and an agent that process starts keeps
        // it for its whole life. A file is opened close-on-exec in one step.
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        let status = std::process::Command::new(self.bins.dir().join(program))
            .args(args)
            .env_clear()
            .envs(self.env())
            .envs(extra.iter().copied())
            .stdin(std::process::Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?)
            .status()?;
        Ok(CliOutput {
            code: status.code(),
            stdout: read_back(&mut stdout)?,
            stderr: read_back(&mut stderr)?,
        })
    }

    /// Every call so far, oldest first.
    pub fn calls(&self) -> Vec<LoggedCall> {
        fsutil::read_text(&self.log_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// Writes a file under the world's root (a settings file, a stub) and
    /// returns its path.
    pub fn write_file(&self, name: &str, text: &str) -> std::io::Result<PathBuf> {
        let path = self.root.path().join(name);
        fsutil::write_atomic(&path, text.as_bytes())?;
        Ok(path)
    }

    /// Adds a session file, as a Claude Code session that is not a
    /// `fake-claude` process would leave (`"kind": "bg"` for a background
    /// session, whose state `claude agents --json` shows under `state`).
    /// Returns the file's path.
    pub fn add_session_file(&self, pid: u32, doc: &Value) -> std::io::Result<PathBuf> {
        let dir = self.claude_dir().join("sessions");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{pid}.json"));
        fsutil::write_atomic(&path, doc.to_string().as_bytes())?;
        Ok(path)
    }

    /// Adds an agent that has no name, as real herdr lists agents it
    /// detected in a pane it did not start (25 of 38 in a live list). It has
    /// its own workspace and pane, no process, and status `idle`. Returns
    /// its `pane_id`.
    pub fn add_unnamed_agent(&self) -> std::io::Result<String> {
        let out = self.run(&["fake", "add-unnamed-agent"])?;
        out.result()
            .and_then(|r| r.pointer("/agent/pane_id")?.as_str().map(str::to_owned))
            .ok_or_else(|| std::io::Error::other(format!("no unnamed agent made: {out:?}")))
    }

    /// How each `permission` step of a `fake-claude` session ended, oldest
    /// first: `{"by": "hook"|"key"|"timeout", "decision": "allow"|"deny",
    /// "message": <text or null>}`, read from `<claude dir>/permission/`.
    pub fn permission_outcomes(&self, session: &str) -> Vec<Value> {
        let dir = self.claude_dir().join("permission");
        (1..)
            .map(|n| dir.join(format!("{session}-{n}.json")))
            .map_while(|path| fsutil::read_text(&path))
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect()
    }

    /// The session files `fake-claude` processes have written and not yet
    /// removed.
    pub fn session_files(&self) -> Vec<PathBuf> {
        // `fake-claude` writes each file through a hidden `.<name>.<pid>.tmp`
        // in the same folder; a caller that saw one would read a half-written file.
        let shown = |p: &PathBuf| {
            p.file_name()
                .is_some_and(|n| !n.to_string_lossy().starts_with('.'))
        };
        std::fs::read_dir(self.claude_dir().join("sessions"))
            .map(|d| {
                d.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(shown)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// How many prompts typed to live agents they have not taken yet. An
    /// agent goes on showing its old status for a moment after a prompt
    /// reaches its inbox, so a test that waits for idle agents waits for
    /// this to be zero first.
    pub fn untaken_prompts(&self) -> usize {
        let state_path = self.state_path();
        self.agent_panes()
            .iter()
            .map(|(pane, pid)| crate::herdr_bin::untaken_prompts(&state_path, pane, *pid))
            .sum()
    }

    /// Each pane the state file gives an agent process, with that process.
    fn agent_panes(&self) -> Vec<(String, u32)> {
        let Some(doc) = fsutil::read_text(&self.state_path())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            return Vec::new();
        };
        let panes = doc.get("panes").and_then(Value::as_array);
        panes
            .into_iter()
            .flatten()
            .filter_map(|pane| {
                let pid = pane
                    .pointer("/agent/pid")
                    .and_then(Value::as_u64)
                    .and_then(|p| u32::try_from(p).ok())?;
                let id = pane
                    .get("pane_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some((id.to_owned(), pid))
            })
            .collect()
    }
}

/// The file whose lock `FakeHerdrEnv` holds, under the world's root.
fn owner_lock_path(root: &Path) -> PathBuf {
    root.join("owner.lock")
}

/// Everything a program wrote to `file`, from the start.
fn read_back(file: &mut std::fs::File) -> std::io::Result<String> {
    use std::io::{Read as _, Seek as _};
    file.rewind()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl Drop for FakeHerdrEnv {
    /// Ends every agent process still running, so a test that ends early
    /// (or a `never_stop` fault) leaves nothing behind.
    fn drop(&mut self) {
        for (pane, pid) in self.agent_panes() {
            crate::herdr_bin::end_agent(&self.state_path(), &pane, pid);
        }
    }
}
