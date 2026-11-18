//! The rig of the `teamwire amend` tests: a team launched by the real
//! `teamwire launch` against `fake-herdr` and `fake-claude`, in a temporary
//! home and a throwaway repository, in the shape of `tests/handoff.rs`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use teamwire_core::{
    CompletionStatus, FinalSource, Member, Message, MessageType, Recipient, Role, TeamName,
    Timestamp,
};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, TestHome};
use teamwire_store::{MessageFilter, NewCompletion, Store};

#[derive(Debug)]
pub struct Rig {
    pub herdr: FakeHerdrEnv,
    pub home: TestHome,
    pub work: tempfile::TempDir,
}

#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Run {
    pub fn all(&self) -> String {
        format!("{}{}", self.out, self.err)
    }
}

pub fn git(dir: &Path, args: &[&str]) -> String {
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

pub fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

fn now() -> Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    Timestamp::from_unix_millis(i64::try_from(millis).unwrap()).unwrap()
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    pub fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    pub fn teamwire(&self, args: &[&str]) -> Run {
        self.teamwire_with(args, None, &[])
    }

    pub fn teamwire_with(&self, args: &[&str], stdin: Option<&str>, extra: &[(&str, &str)]) -> Run {
        use std::io::Write as _;
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
        cmd.args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .envs(extra.iter().copied())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(stdin.unwrap_or_default().as_bytes()).ok();
        drop(input);
        let out = child.wait_with_output().unwrap();
        Run {
            code: out.status.code().unwrap(),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// A rig whose `fake-claude` members follow `script`.
    pub fn with_script(script: &ClaudeScript) -> Self {
        let mut rig = Self::new();
        rig.herdr.set_script(script).unwrap();
        rig
    }

    /// Launches team `alpha` with the worktree implementer `api`, whose
    /// check is `check` and owns `owns`, from a repository that holds
    /// `files`. Waits for its first turn to end. Returns its worktree.
    pub fn launch(&self, check: &str, owns: &[&str], files: &[(&str, &str)]) -> PathBuf {
        self.launch_until(check, owns, files, "done")
    }

    /// `launch`, waiting until `api`'s pane reads `status` instead: a
    /// script that stops at a dialog never ends its first turn.
    pub fn launch_until(
        &self,
        check: &str,
        owns: &[&str],
        files: &[(&str, &str)],
        status: &str,
    ) -> PathBuf {
        let repo = self.work.path().join("proj");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join(".gitignore"), ".worktrees/\n").unwrap();
        for (path, text) in files {
            let path = repo.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "first"]);
        let spec = json!({"team": "alpha", "title": "Alpha", "children": [
            {"role": "api", "title": "API work", "mode": "worktree", "repo": repo,
             "task": "build", "check": check, "owns": owns}]});
        let run = self.teamwire_with(&["launch"], Some(&spec.to_string()), &[]);
        assert_eq!(run.code, 0, "launch: {}", run.all());
        self.wait_for_status(status);
        repo.join(".worktrees/api")
    }

    pub fn store(&self) -> Store {
        Store::open(self.home.path(), &TeamName::parse("alpha").unwrap()).unwrap()
    }

    pub fn member(&self, name: &str) -> Member {
        self.store()
            .read(|tx| tx.member(&role(name)))
            .unwrap()
            .unwrap()
    }

    /// The notes from the lead to `name`, oldest first.
    pub fn lead_notes_to(&self, name: &str) -> Vec<Message> {
        let filter = MessageFilter {
            sender: Some(teamwire_core::Actor::Lead),
            recipient: Some(Recipient::Member(role(name))),
            kind: Some(MessageType::Note),
            ..MessageFilter::default()
        };
        self.store()
            .read(|tx| tx.messages(&filter)?.all_or_corrupt())
            .unwrap()
    }

    /// `api`'s row as `teamwire teams --json` prints it.
    pub fn api_json(&self) -> Value {
        let run = self.teamwire(&["teams", "--json"]);
        assert_eq!(run.code, 0, "{}", run.all());
        let json: Value = serde_json::from_str(&run.out).unwrap();
        json["teams"][0]["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "api")
            .cloned()
            .unwrap()
    }

    /// Records `api`'s result of its current generation, as `teamwire done`
    /// and the turn-end hook that delivers it would.
    pub fn record_done(&self, summary: &str) {
        let generation = self.member("api").generation;
        let mut store = self.store();
        store
            .write(|tx| {
                let c = tx.insert_completion(
                    now(),
                    &NewCompletion {
                        role: role("api"),
                        generation,
                        status: CompletionStatus::Done,
                        summary: summary.to_owned(),
                        final_message: None,
                        final_source: FinalSource::None,
                    },
                )?;
                tx.deliver_completion_via_watch(now(), &c.ulid)?;
                Ok(())
            })
            .unwrap();
    }

    /// Points `api`'s row at a session no process runs, as when its process
    /// is gone.
    pub fn lose_process(&self) {
        let session = teamwire_core::SessionId::parse("no-such-session").unwrap();
        self.store()
            .write(|tx| tx.set_member_session(&role("api"), &session))
            .unwrap();
    }

    /// Writes `api`'s report with a progress line for each of `shas`.
    pub fn write_report(&self, shas: &[&str]) {
        let lines: Vec<String> = shas
            .iter()
            .map(|s| format!("- {} work | left: none", &s[..7]))
            .collect();
        let dir = self.home.path().join("teams/alpha/reports");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("api.md"),
            format!(
                "Summary: done.\n\n{}\n\nDecisions I made: none\n",
                lines.join("\n")
            ),
        )
        .unwrap();
    }

    /// The file through which `fake-herdr` reads the pane's agent status.
    fn status_file(&self) -> PathBuf {
        self.herdr
            .root()
            .join("fake-herdr-panes")
            .join(self.member("api").place.unwrap().id.replace(':', "_"))
            .join("status.json")
    }

    pub fn wait_for_status(&self, want: &str) {
        let file = self.status_file();
        let reached = (0..1500).any(|_| {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let at = serde_json::from_str::<Value>(&text)
                .is_ok_and(|s| s.get("status").is_some_and(|v| v == want));
            if !at {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            at
        });
        assert!(reached, "api's pane never read {want}");
    }

    /// Waits until the agent has taken every line typed into its pane and
    /// reads `done` or `idle`.
    pub fn wait_until_settled(&self) {
        let file = self.status_file();
        let inbox = file.with_file_name("inbox");
        let settled = (0..1500).any(|_| {
            let typed = std::fs::read_to_string(&inbox).map_or(0, |t| t.lines().count());
            let status = std::fs::read_to_string(&file)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok());
            let at = status.is_some_and(|s| {
                let taken = s.get("prompts").and_then(Value::as_u64).unwrap_or(0);
                let idle = s.get("status").is_some_and(|v| v == "done" || v == "idle");
                idle && usize::try_from(taken).is_ok_and(|n| n >= typed)
            });
            if !at {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            at
        });
        assert!(settled, "api never took every typed line and settled");
    }

    /// The herdr calls made since call number `from`, as "<command> <args>".
    pub fn calls_since(&self, from: usize) -> Vec<String> {
        self.herdr.calls()[from..]
            .iter()
            .map(|c| c.argv.join(" "))
            .collect()
    }
}
