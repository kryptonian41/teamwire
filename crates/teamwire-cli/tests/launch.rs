//! `teamwire launch` through the real binary, with `fake-herdr` and
//! `fake-claude` first on `PATH`, a temporary home and a throwaway repository.
//! The exit codes are those of `docs/api/cli.md`.

#![cfg(test)]
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};
use teamwire_core::{MemberState, Role, TeamName};
use teamwire_fakes::{FakeHerdrEnv, HerdrFaults, TestHome};
use teamwire_store::Store;

struct Rig {
    herdr: FakeHerdrEnv,
    home: TestHome,
    work: tempfile::TempDir,
}

#[expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and git in a throwaway repository"
)]
fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    std::process::Command::new(program)
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test writes its own fixture files"
)]
fn write_file(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

impl Rig {
    fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    /// Runs `teamwire launch` with a cleared environment: the fakes' `PATH`,
    /// a temporary home, and `HERDR_ENV=1` unless `no_herdr`.
    fn launch(&self, args: &[&str], stdin: Option<&str>, no_herdr: bool) -> Output {
        let mut cmd = command(env!("CARGO_BIN_EXE_teamwire"));
        cmd.arg("launch")
            .args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if !no_herdr {
            cmd.env("HERDR_ENV", "1");
        }
        let mut child = cmd.spawn().unwrap();
        if let Some(text) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        } else {
            drop(child.stdin.take());
        }
        child.wait_with_output().unwrap()
    }

    fn spec_file(&self, spec: &Value) -> PathBuf {
        let path = self.work.path().join("spec.json");
        write_file(&path, &spec.to_string());
        path
    }

    fn home_is_empty(&self) -> bool {
        std::fs::read_dir(self.home.path())
            .unwrap()
            .next()
            .is_none()
    }

    fn repo(&self) -> PathBuf {
        let repo = self.work.path().join("proj");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let out = command("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q", "-b", "main"]);
        write_file(&repo.join(".gitignore"), ".worktrees/\n");
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "first"]);
        repo
    }
}

fn pane_spec(team: &str, cwd: &Path) -> Value {
    json!({"team": team, "title": "Demo team", "children": [
        {"role": "a", "title": "Alpha", "mode": "pane", "cwd": cwd, "task": "look around"}]})
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn bad_json_is_a_refusal_and_creates_nothing() {
    let rig = Rig::new();
    let out = rig.launch(&[], Some("{ not json"), false);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("bad_json"),
        "{}",
        text(&out.stderr)
    );
    assert!(rig.home_is_empty());
}

#[test]
fn a_hyphen_in_the_team_name_is_refused_with_exit_1() {
    let rig = Rig::new();
    let spec = pane_spec("my-team", rig.work.path());
    let out = rig.launch(&[], Some(&spec.to_string()), false);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("bad_name"));
    assert!(rig.home_is_empty());
}

#[test]
fn a_member_on_host_none_is_refused_and_nothing_is_created() {
    let rig = Rig::new();
    let file = rig.spec_file(&pane_spec("demo", rig.work.path()));
    let out = rig.launch(&["--spec", file.to_str().unwrap()], None, true);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(err.contains("[a]") && err.contains("no_host"), "{err}");
    assert!(err.contains("nothing was created"));
    assert!(rig.home_is_empty());
}

#[test]
fn several_refusals_are_printed_together() {
    let rig = Rig::new();
    let spec = json!({"team": "demo", "title": "x", "children": [
        {"role": "a", "title": "Same", "mode": "pane", "cwd": rig.work.path(), "task": "t"},
        {"role": "b", "title": "same", "kind": "implementer", "mode": "pane",
         "cwd": rig.work.path(), "task": "t"}]});
    let out = rig.launch(&[], Some(&spec.to_string()), false);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(
        err.contains("title_repeated") && err.contains("implementer_needs_worktree"),
        "{err}"
    );
}

#[test]
fn dry_run_prints_the_plan_and_touches_nothing() {
    let rig = Rig::new();
    let repo = rig.repo();
    let spec = json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "api", "title": "API work", "mode": "worktree", "repo": repo,
         "task": "build", "check": "true", "owns": ["src/"]}]});
    let file = rig.spec_file(&spec);
    let out = rig.launch(
        &["--dry-run", "--spec", file.to_str().unwrap()],
        None,
        false,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(
        shown.contains("would launch team demo") && shown.contains("api"),
        "{shown}"
    );
    assert!(shown.contains(".worktrees/api"), "{shown}");
    assert!(rig.home_is_empty(), "a dry run creates no state");
    assert!(!repo.join(".worktrees").exists(), "and no worktree");
}

#[test]
fn a_launch_that_fails_is_exit_2_with_the_member_failed_and_cleaned_up() {
    let mut rig = Rig::new();
    // Whatever the adapters are, starting the agent fails.
    rig.herdr
        .set_faults(&HerdrFaults::new().error("agent start", None, None, "agent_pane_busy"))
        .unwrap();
    let repo = rig.repo();
    let spec = json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "api", "title": "API work", "mode": "worktree", "repo": repo,
         "task": "build", "check": "true", "owns": ["src/"]}]});
    let out = rig.launch(&[], Some(&spec.to_string()), false);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{} / {}",
        text(&out.stdout),
        text(&out.stderr)
    );
    let shown = text(&out.stdout);
    assert!(shown.contains("api: failed:"), "{shown}");
    assert!(shown.contains("0/1 members started"), "{shown}");
    assert!(shown.contains("watch: teamwire watch demo"), "{shown}");
    assert!(
        !repo.join(".worktrees/api").exists(),
        "the checkout was taken back"
    );

    let store = Store::open_read_only(rig.home.path(), &TeamName::parse("demo").unwrap()).unwrap();
    let role = Role::parse("api").unwrap();
    let member = store.read(|tx| tx.member(&role)).unwrap().unwrap();
    assert_eq!(member.state, MemberState::Failed);
    for stream in [&out.stdout, &out.stderr] {
        assert!(!text(stream).contains("TEAMWIRE_TOKEN"));
    }
}
