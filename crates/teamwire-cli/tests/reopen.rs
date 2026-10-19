//! `teamwire reopen` through the real binary, with `fake-herdr` and
//! `fake-claude` first on `PATH` and a temporary home. Exit codes and output
//! lines are those of `docs/api/cli.md`.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;
#[path = "support/schema_1.rs"]
mod schema_1;

use std::io::Write as _;
use std::path::Path;
use std::process::Output;

use serde_json::json;
use teamwire_core::{Role, TeamName};
use teamwire_fakes::{FakeHerdrEnv, TestHome};
use teamwire_store::Store;

/// A pid no process has, so a session file naming it is a session that ended.
const GONE: u32 = 2_000_000_000;

struct Rig {
    herdr: FakeHerdrEnv,
    home: TestHome,
    work: tempfile::TempDir,
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Rig {
    fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    /// Runs the binary as the session `caller`, in a cleared environment.
    fn teamwire(&self, args: &[&str], caller: &str, stdin: Option<&str>) -> Output {
        self.teamwire_with(args, caller, stdin, &[])
    }

    /// `teamwire` with extra variables, such as a member's identity.
    fn teamwire_with(
        &self,
        args: &[&str],
        caller: &str,
        stdin: Option<&str>,
        vars: &[(&str, &str)],
    ) -> Output {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
        cmd.args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("HERDR_ENV", "1")
            .env("TEAMWIRE_HOME", self.home.path())
            .env("CLAUDE_CODE_SESSION_ID", caller)
            .envs(vars.iter().copied())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(stdin.unwrap_or_default().as_bytes()).ok();
        drop(input);
        child.wait_with_output().unwrap()
    }

    /// The lead's session as Claude Code leaves it when it has ended.
    fn lead_session_file(&self, id: &str, cwd: &Path) {
        let doc = json!({ "pid": GONE, "sessionId": id, "cwd": cwd, "kind": "interactive" });
        self.herdr.add_session_file(GONE, &doc).unwrap();
    }

    /// Launches team `demo` with member `a`, then closes member `a`'s tab:
    /// every session of the team is gone.
    fn closed_team(&self, lead: &str) {
        self.lead_session_file(lead, self.work.path());
        let spec = json!({"team": "demo", "title": "Demo", "children": [
            {"role": "a", "title": "Alpha", "mode": "pane", "cwd": self.work.path(),
             "task": "look around"}]});
        let out = self.teamwire(&["launch"], lead, Some(&spec.to_string()));
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        let store =
            Store::open_read_only(self.home.path(), &TeamName::parse("demo").unwrap()).unwrap();
        let member = store
            .read(|tx| tx.member(&Role::parse("a").unwrap()))
            .unwrap()
            .unwrap();
        let place = member.place.unwrap();
        self.herdr.run(&["pane", "close", &place.id]).unwrap();
    }
}

#[test]
fn an_unknown_team_is_refused_with_exit_1() {
    let rig = Rig::new();
    let out = rig.teamwire(&["reopen", "nosuch"], "lead-sess", None);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("nosuch"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn the_lead_and_the_dead_member_come_back_and_exit_is_0() {
    let rig = Rig::new();
    rig.closed_team("lead-sess");
    let out = rig.teamwire(&["reopen", "demo"], "other-sess", None);
    let shown = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{shown} / {}",
        text(&out.stderr)
    );
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(lines[..2], ["lead reopened", "a reopened"], "{shown}");
    assert!(shown.contains("watch: teamwire watch demo"), "{shown}");
    assert!(shown.contains("reprompt or a note"), "{shown}");
    // The host started both agents, the lead first, each with a resume.
    let starts: Vec<String> = rig
        .herdr
        .calls()
        .into_iter()
        .filter(|c| c.command() == "agent start")
        .map(|c| c.argv.join(" "))
        .collect();
    let resumes: Vec<&String> = starts.iter().filter(|a| a.contains("--resume")).collect();
    assert_eq!(resumes.len(), 2, "{starts:?}");
    assert!(resumes[0].contains("tw_demo_lead"), "{resumes:?}");
    assert!(resumes[1].contains("tw_demo_a"), "{resumes:?}");
}

#[test]
fn a_caller_that_is_the_lead_is_skipped_by_session_id() {
    let rig = Rig::new();
    rig.closed_team("lead-sess");
    let out = rig.teamwire(&["reopen", "demo"], "lead-sess", None);
    let shown = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{shown} / {}",
        text(&out.stderr)
    );
    assert!(
        shown.starts_with("lead skipped: already open (this session)\na reopened\n"),
        "{shown}"
    );
    assert!(!shown.contains("lead --adopt"), "{shown}");
}

#[test]
fn a_caller_that_is_not_the_lead_is_told_how_to_adopt() {
    let rig = Rig::new();
    rig.closed_team("lead-sess");
    let out = rig.teamwire(&["reopen", "demo", "--dry-run"], "other-sess", None);
    let shown = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{shown} / {}",
        text(&out.stderr)
    );
    assert!(shown.contains("results go to the reopened lead"), "{shown}");
    assert!(shown.contains("teamwire lead --adopt demo"), "{shown}");
}

#[test]
fn a_member_session_cannot_reopen_a_team() {
    let rig = Rig::new();
    rig.closed_team("lead-sess");
    let db = teamwire_store::database_path(rig.home.path(), &TeamName::parse("demo").unwrap());
    let before = (rig.herdr.calls().len(), std::fs::read(&db).unwrap());
    for vars in [
        &[("TEAMWIRE_TOKEN", "not-a-real-token")][..],
        &[("TEAMWIRE_TEAM", "demo"), ("TEAMWIRE_ROLE", "a")],
    ] {
        for args in [&["reopen", "demo"][..], &["reopen", "demo", "--dry-run"]] {
            let out = rig.teamwire_with(args, "other-sess", None, vars);
            let err = text(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{vars:?} {args:?}: {err}");
            assert!(err.contains("refused.identity"), "{err}");
        }
    }
    let after = (rig.herdr.calls().len(), std::fs::read(&db).unwrap());
    assert!(
        after == before,
        "a refused reopen asked the host or wrote the store"
    );
}

#[test]
fn a_dry_run_opens_nothing_and_writes_nothing() {
    let rig = Rig::new();
    rig.closed_team("lead-sess");
    let before = rig.herdr.calls().len();
    let out = rig.teamwire(&["reopen", "demo", "--dry-run"], "other-sess", None);
    let shown = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{shown}");
    assert!(
        shown.starts_with("lead would reopen\na would reopen\n"),
        "{shown}"
    );
    assert!(shown.contains("dry run"), "{shown}");
    let started = rig.herdr.calls()[before..]
        .iter()
        .filter(|c| c.command().contains("start"))
        .count();
    assert_eq!(started, 0, "a dry run starts no agent");
}

#[test]
fn a_refused_session_makes_the_exit_2() {
    let rig = Rig::new();
    rig.closed_team("lead-sess");
    std::fs::remove_dir_all(rig.home.path().join("teams/demo/members/a")).unwrap();
    let out = rig.teamwire(&["reopen", "demo", "--role", "a"], "lead-sess", None);
    let shown = text(&out.stdout);
    assert_eq!(out.status.code(), Some(2), "{shown}");
    assert!(shown.starts_with("a refused: its file "), "{shown}");
}

#[test]
fn a_dry_run_leaves_an_older_database_byte_for_byte() {
    let rig = Rig::new();
    let db = schema_1::version_1_team(rig.home.path(), "old", 1).unwrap();
    let before = std::fs::read(&db).unwrap();
    let out = rig.teamwire(&["reopen", "old", "--dry-run"], "lead-sess", None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(std::fs::read(&db).unwrap(), before, "the dry run migrated");
}

#[test]
fn results_the_lead_never_received_are_listed_with_their_paths() {
    let mut bed = fixture::Bed::new();
    bed.add("api");
    let done = bed.complete("api", teamwire_core::CompletionStatus::Done);
    bed.finish(&done, teamwire_core::ReportState::Undelivered, None);
    let run = |extra: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .args(["--home"])
            .arg(bed.home.path())
            .args(["reopen", "alpha"])
            .args(extra)
            .env_clear()
            .env("TEAMWIRE_HOME", bed.home.path())
            .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
            .output()
            .unwrap();
        text(&out.stdout)
    };
    for shown in [run(&["--dry-run"]), run(&["--role", "api"])] {
        assert!(
            shown.contains("results the lead has not received:"),
            "{shown}"
        );
        assert!(shown.contains("api #1 undelivered "), "{shown}");
        assert!(
            shown.contains("the lead should read those files"),
            "{shown}"
        );
    }
}
