//! `teamwire lead --handoff` through the real binary (ADR 0051): a lead
//! session run by `fake-claude` in a `fake-herdr` pane hands its team to a
//! fresh session in a pane beside it.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and write the files a lead would"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use teamwire_core::{HarnessId, HostId, LeadRoute, Pid, SessionId, SessionKind, TeamName};
use teamwire_fakes::{FakeHerdrEnv, TestHome};
use teamwire_store::{NewTeam, Store};

const OLD: &str = "6f1c2d3e-0000-4000-8000-0000000000aa";

struct Rig {
    herdr: FakeHerdrEnv,
    home: TestHome,
    work: tempfile::TempDir,
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Rig {
    fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    /// Runs the binary as the old lead: inside herdr, with the lead's
    /// session in its environment.
    fn teamwire(&self, args: &[&str]) -> Run {
        self.teamwire_as(OLD, args)
    }

    /// Runs the binary as the Claude Code session `session`.
    fn teamwire_as(&self, session: &str, args: &[&str]) -> Run {
        self.teamwire_env(session, &[], args)
    }

    /// `teamwire_as`, with `extra` added to the environment.
    fn teamwire_env(&self, session: &str, extra: &[(&str, &str)], args: &[&str]) -> Run {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .env("CLAUDE_CODE_SESSION_ID", session)
            .envs(extra.iter().copied())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        Run {
            code: out.status.code().unwrap(),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// Team `alpha`, led by the old session on Claude Code in herdr.
    fn team(&self) {
        let name = TeamName::parse("alpha").unwrap();
        let mut store = Store::open(self.home.path(), &name).unwrap();
        let team = NewTeam {
            name,
            title: "Alpha work".to_owned(),
            lead_harness: HarnessId::from_static("claude-code"),
            lead_host: HostId::from_static("herdr"),
            lead_route: LeadRoute::Sink,
            lead_session: Some((SessionId::parse(OLD).unwrap(), SessionKind::Interactive)),
        };
        let at = teamwire_core::Timestamp::from_unix_millis(1_800_000_000_000).unwrap();
        let cwd = json!({ "lead_cwd": self.work.path() });
        store
            .write(|tx| tx.create_team_with(at, &team, &cwd).map(|_| ()))
            .unwrap();
    }

    /// The old lead: `fake-claude` with the old session, started in a pane
    /// the way a user starts a lead, and its handoff document.
    fn old_lead(&self) -> String {
        let made = self
            .herdr
            .run(&[
                "workspace",
                "create",
                "--cwd",
                &self.work.path().display().to_string(),
                "--no-focus",
            ])
            .unwrap();
        let pane = made.result().unwrap()["root_pane"]["pane_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let ran = self
            .herdr
            .run(&[
                "pane",
                "run",
                &pane,
                &format!("claude --session-id {OLD} --verbose"),
            ])
            .unwrap();
        assert_eq!(ran.code, Some(0), "{ran:?}");
        self.pid_of(OLD);
        let dir = self.home.path().join("handoffs").join(OLD);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("handoff.md"), "# handoff\n").unwrap();
        pane
    }

    /// The pid of the `fake-claude` process that runs `session`, once its
    /// session file is written.
    fn pid_of(&self, session: &str) -> Pid {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let found = self.herdr.session_files().iter().find_map(|f| {
                let doc: Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
                (doc["sessionId"] == session).then(|| doc["pid"].as_u64())?
            });
            if let Some(pid) = found {
                return Pid::new(u32::try_from(pid).unwrap());
            }
            assert!(Instant::now() < until, "no session file for {session}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    fn lead(&self) -> SessionId {
        Store::open_read_only(self.home.path(), &TeamName::parse("alpha").unwrap())
            .unwrap()
            .read(|tx| tx.team())
            .unwrap()
            .unwrap()
            .lead_session
            .unwrap()
    }

    /// The `herdr` calls of one command, such as `pane split`, by argv.
    fn calls_of(&self, command: &str) -> Vec<Vec<String>> {
        let words: Vec<&str> = command.split(' ').collect();
        self.herdr
            .calls()
            .into_iter()
            .map(|c| c.argv)
            .filter(|a| a.len() >= words.len() && a[..words.len()] == words[..])
            .collect()
    }

    fn document(&self) -> PathBuf {
        self.home
            .path()
            .join("handoffs")
            .join(OLD)
            .join("handoff.md")
    }
}

#[test]
fn the_old_lead_hands_its_team_to_a_fresh_claude_session_in_a_pane_beside_it() {
    let rig = Rig::new();
    rig.team();
    let old_pane = rig.old_lead();

    let run = rig.teamwire(&["lead", "--handoff"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);

    let new = rig.lead();
    assert_ne!(new.as_str(), OLD);
    let splits = rig.calls_of("pane split");
    assert_eq!(splits.len(), 1, "{splits:?}");
    assert_eq!(splits[0][2], old_pane);
    assert!(
        splits[0]
            .windows(2)
            .any(|w| w[0] == "--env"
                && w[1] == format!("TEAMWIRE_HOME={}", rig.home.path().display())),
        "{splits:?}"
    );
    let renames = rig.calls_of("pane rename");
    assert_eq!(renames.len(), 1, "{renames:?}");
    assert_eq!(renames[0][3], "lead \u{b7} Alpha work");
    assert_eq!(
        rig.calls_of("pane close"),
        [["pane", "close", old_pane.as_str()]]
    );

    // The new session runs with the old lead's carried flag and the first
    // prompt as its last argument.
    let pid = rig.pid_of(new.as_str());
    let args = teamwire_platform::native().args_of(pid).unwrap();
    assert_eq!(args[1..4], ["--session-id", new.as_str(), "--verbose"]);
    let prompt = args.last().unwrap();
    assert!(
        prompt.starts_with(&format!(
            "You are a teamwire lead. You take over from lead session {OLD}, which handed off \
             because its context was full. You now lead these teams: alpha. Load the \
             `teamwire:teamwire` skill with your Skill tool. Then read {} in full.",
            rig.document().display()
        )),
        "{prompt}"
    );
    assert!(
        run.out
            .contains(&format!("session {OLD} is no longer the lead")),
        "{}",
        run.out
    );

    // The new lead's catch-up list reads the same store.
    let missed = rig.teamwire_as(new.as_str(), &["lead", "--missed"]);
    assert_eq!(missed.code, 0, "{}{}", missed.out, missed.err);
    assert_eq!(
        missed.out,
        "nothing was missed: no result or message reached the old lead while it handed off\n"
    );
}

#[test]
fn the_guide_names_the_callers_document_and_teams_and_carries_the_licence() {
    let rig = Rig::new();
    rig.team();
    let run = rig.teamwire(&["lead", "--handoff-guide"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for want in [
        "your teams: alpha.".to_owned(),
        format!(
            "Write the document to {}, never into a repository.",
            rig.document().display()
        ),
        "1. **The goal**".to_owned(),
        "4. **Handled so far**".to_owned(),
        "8. **Pull requests**".to_owned(),
        "10. **Skills to load**".to_owned(),
        "Redact secrets".to_owned(),
        "run `teamwire lead --handoff` as the last action of your turn".to_owned(),
        "give this command a limit of\nat least 240 seconds".to_owned(),
        "mattpocock-skills 1.3.1".to_owned(),
        "MIT License".to_owned(),
        "Copyright (c) 2026 Matt Pocock".to_owned(),
    ] {
        assert!(run.out.contains(&want), "{want} not in:\n{}", run.out);
    }
    assert!(!run.out.contains("{{"), "{}", run.out);
    // A session that leads no open team has no handoff to write.
    let other = rig.teamwire_as(
        "6f1c2d3e-0000-4000-8000-0000000000bb",
        &["lead", "--handoff-guide"],
    );
    assert_eq!(other.code, 1, "{}{}", other.out, other.err);
    assert!(other.err.contains("leads no open team"), "{}", other.err);
}

#[test]
fn a_dry_run_names_the_plan_and_changes_nothing() {
    let rig = Rig::new();
    rig.team();
    let old_pane = rig.old_lead();

    let run = rig.teamwire(&["lead", "--handoff", "--dry-run"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for want in [
        "teams: alpha".to_owned(),
        format!("old pane: {old_pane} (session {OLD})"),
        format!("handoff document: {}", rig.document().display()),
        "command: claude --session-id ".to_owned(),
        "dry run: no pane was opened and no team changed".to_owned(),
    ] {
        assert!(run.out.contains(&want), "{want} not in:\n{}", run.out);
    }
    assert!(rig.calls_of("pane split").is_empty());
    assert_eq!(rig.lead().as_str(), OLD);

    // The new pane gets the config folder of the lead's own harness only.
    let run = rig.teamwire_env(
        OLD,
        &[
            ("CLAUDE_CONFIG_DIR", "/cfg/claude"),
            ("CODEX_HOME", "/cfg/codex"),
        ],
        &["lead", "--handoff", "--dry-run"],
    );
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out
            .contains("environment: CLAUDE_CONFIG_DIR=/cfg/claude"),
        "{}",
        run.out
    );
    assert!(!run.out.contains("CODEX_HOME"), "{}", run.out);
}

#[test]
fn a_handoff_without_its_document_is_refused_with_exit_1() {
    let rig = Rig::new();
    rig.team();
    rig.old_lead();
    std::fs::remove_file(rig.document()).unwrap();

    let run = rig.teamwire(&["lead", "--handoff"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(
        run.err.contains("teamwire lead --handoff-guide"),
        "{}",
        run.err
    );
    assert!(rig.calls_of("pane split").is_empty());
}

#[test]
fn lead_takes_exactly_one_action_and_dry_run_only_with_handoff() {
    let rig = Rig::new();
    for args in [
        &["lead"][..],
        &["lead", "--adopt", "alpha", "--handoff"],
        &["lead", "--adopt", "alpha", "--dry-run"],
        &["lead", "--handoff-guide", "--dry-run"],
        &["lead", "--missed", "--handoff"],
    ] {
        let run = rig.teamwire(args);
        assert_eq!(run.code, 3, "{args:?}: {}{}", run.out, run.err);
    }
}
