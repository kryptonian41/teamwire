//! `teamwire handoff` through the real binary: a team launched by the real
//! `teamwire launch` against `fake-herdr` and `fake-claude`, in a temporary
//! home and a throwaway repository.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and git in a throwaway repository"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "helper functions in an integration-test file unwrap, index and panic; a failure there fails the test"
)]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use teamwire_core::{Member, Role, TeamName};
use teamwire_fakes::{FakeHerdrEnv, TestHome};
use teamwire_store::Store;

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

fn git(dir: &Path, args: &[&str]) -> String {
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

impl Rig {
    fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    fn teamwire(&self, args: &[&str], stdin: Option<&str>) -> Run {
        self.teamwire_with(args, stdin, &[])
    }

    fn teamwire_with(&self, args: &[&str], stdin: Option<&str>, extra: &[(&str, &str)]) -> Run {
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

    /// Launches team `alpha` with the worktree implementer `api`, and waits
    /// for its first turn to end, so its pane reads idle.
    fn launch(&self) -> PathBuf {
        let repo = self.work.path().join("proj");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join(".gitignore"), ".worktrees/\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "first"]);
        let spec = json!({"team": "alpha", "title": "Alpha", "children": [
            {"role": "api", "title": "API work", "mode": "worktree", "repo": repo,
             "task": "build", "check": "true", "owns": ["src/"]}]});
        let run = self.teamwire(&["launch"], Some(&spec.to_string()));
        assert_eq!(run.code, 0, "launch: {}{}", run.out, run.err);
        self.wait_for_status("done");
        repo.join(".worktrees/api")
    }

    fn api(&self) -> Member {
        Store::open_read_only(self.home.path(), &TeamName::parse("alpha").unwrap())
            .unwrap()
            .read(|tx| tx.member(&Role::parse("api").unwrap()))
            .unwrap()
            .unwrap()
    }

    fn handoff_events(&self) -> Vec<Value> {
        Store::open_read_only(self.home.path(), &TeamName::parse("alpha").unwrap())
            .unwrap()
            .read(|tx| {
                tx.events_of_role(&Role::parse("api").unwrap())?
                    .all_or_corrupt()
            })
            .unwrap()
            .into_iter()
            .filter(|e| e.kind.as_str() == "member.handoff")
            .map(|e| e.data)
            .collect()
    }

    /// The file through which `fake-herdr` reads the pane's agent status.
    fn status_file(&self) -> PathBuf {
        self.herdr
            .root()
            .join("fake-herdr-panes")
            .join(self.api().place.unwrap().id.replace(':', "_"))
            .join("status.json")
    }

    fn wait_for_status(&self, want: &str) {
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

    fn set_status(&self, status: &str) {
        std::fs::write(
            self.status_file(),
            format!(r#"{{"status":"{status}","seq":99}}"#),
        )
        .unwrap();
    }

    /// The herdr calls made since call number `from`, as "<command> <args>".
    fn calls_since(&self, from: usize) -> Vec<String> {
        self.herdr.calls()[from..]
            .iter()
            .map(|c| c.argv.join(" "))
            .collect()
    }
}

#[test]
fn the_guide_needs_no_team_and_carries_the_document_path_the_summary_and_the_licence() {
    let rig = Rig::new();
    let run = rig.teamwire(&["handoff", "--guide"], None);
    assert_eq!(run.code, 0, "{}", run.err);
    for wanted in [
        "reports/<your role>.handoff.md",
        "handoff:",
        "mattpocock-skills 1.3.1",
        "MIT License",
        "Copyright (c) 2026 Matt Pocock",
        "Permission is hereby granted",
        "THE SOFTWARE IS PROVIDED \"AS IS\"",
    ] {
        assert!(run.out.contains(wanted), "{wanted}: {}", run.out);
    }
    assert!(!run.out.contains("$TMPDIR`,"), "never the temp folder");
    assert!(run.out.lines().count() < 60);
}

#[test]
fn an_idle_member_gets_a_fresh_session_in_a_new_tab_and_its_first_line() {
    let rig = Rig::new();
    let worktree = rig.launch();
    let before = rig.api();
    let old_place = before.place.clone().unwrap();
    let from = rig.herdr.calls().len();

    let run = rig.teamwire(&["handoff", "alpha", "--role", "api"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);

    let after = rig.api();
    let new = after.session_id.clone().unwrap();
    assert_ne!(Some(&new), before.session_id.as_ref());
    assert_ne!(after.place, before.place);
    assert_eq!(after.generation, before.generation + 1);
    assert_eq!(after.cwd, before.cwd);
    assert_eq!(after.branch, before.branch);
    let events = rig.handoff_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["new_session"], new.as_str());

    let calls = rig.calls_since(from);
    let closed = calls
        .iter()
        .position(|c| c == &format!("pane close {}", old_place.id))
        .unwrap_or_else(|| panic!("the old pane was not closed: {calls:?}"));
    let started = calls
        .iter()
        .position(|c| c.starts_with("agent start"))
        .unwrap_or_else(|| panic!("no agent started: {calls:?}"));
    assert!(closed < started, "the old place closes first: {calls:?}");
    let start = &calls[started];
    assert!(start.contains(&format!("--session-id {new}")), "{start}");
    assert!(start.contains("members/api/settings.json"), "{start}");
    assert!(start.contains("members/api/mcp.json"), "{start}");
    let prompt = calls
        .iter()
        .find(|c| c.starts_with("agent prompt"))
        .unwrap_or_else(|| panic!("no first line: {calls:?}"));
    assert!(
        prompt.contains(&worktree.join("TASK.md").display().to_string()),
        "{prompt}"
    );
    assert!(prompt.contains("reports/api.handoff.md"), "{prompt}");
    assert!(
        !calls.iter().any(|c| c.contains("reprompt")),
        "no reprompt: {calls:?}"
    );
    assert!(
        run.out.contains("api: handoff recorded, generation"),
        "{}",
        run.out
    );
}

#[test]
fn a_busy_member_and_uncommitted_work_are_refused_with_nothing_changed() {
    let rig = Rig::new();
    let worktree = rig.launch();
    let before = rig.api();
    let from = rig.herdr.calls().len();

    rig.set_status("working");
    let busy = rig.teamwire(&["handoff", "alpha", "--role", "api"], None);
    assert_eq!(busy.code, 1, "{}{}", busy.out, busy.err);
    assert!(busy.err.contains("api is busy"), "{}", busy.err);

    rig.set_status("idle");
    std::fs::write(worktree.join("wip.txt"), "not committed\n").unwrap();
    let dirty = rig.teamwire(&["handoff", "alpha", "--role", "api"], None);
    assert_eq!(dirty.code, 1, "{}{}", dirty.out, dirty.err);
    assert!(dirty.err.contains("1 uncommitted path"), "{}", dirty.err);

    assert_eq!(rig.api(), before);
    assert!(rig.handoff_events().is_empty());
    let calls = rig.calls_since(from);
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("pane close") || c.starts_with("agent start")),
        "{calls:?}"
    );
}

#[test]
fn a_dry_run_prints_the_steps_and_changes_nothing() {
    let rig = Rig::new();
    rig.launch();
    let before = rig.api();
    let db = teamwire_store::database_path(rig.home.path(), &TeamName::parse("alpha").unwrap());
    let bytes = std::fs::read(&db).unwrap();
    let from = rig.herdr.calls().len();

    let run = rig.teamwire(&["handoff", "alpha", "--role", "api", "--dry-run"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("api: would close place"), "{}", run.out);
    assert!(
        run.out.contains("api: would open tw_alpha_api"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains("dry run: nothing was closed"),
        "{}",
        run.out
    );
    assert_eq!(rig.api(), before);
    assert_eq!(std::fs::read(&db).unwrap(), bytes);
    let calls = rig.calls_since(from);
    assert!(
        !calls.iter().any(|c| c.starts_with("pane close")
            || c.starts_with("agent start")
            || c.starts_with("agent prompt")),
        "{calls:?}"
    );
}

#[test]
fn a_member_session_cannot_hand_off_and_a_missing_team_or_role_is_a_usage_error() {
    let rig = Rig::new();
    let run = rig.teamwire_with(
        &["handoff", "alpha", "--role", "api"],
        None,
        &[("TEAMWIRE_TOKEN", "not-a-real-token")],
    );
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("refused.identity"), "{}", run.err);
    for args in [
        &["handoff"][..],
        &["handoff", "alpha"],
        &["handoff", "--role", "api"],
    ] {
        assert_eq!(rig.teamwire(args, None).code, 3, "{args:?}");
    }
}
