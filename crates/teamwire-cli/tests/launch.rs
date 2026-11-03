//! `teamwire launch` through the real binary, with `fake-herdr` and
//! `fake-claude` first on `PATH`, a temporary home and a throwaway repository.
//! The exit codes are those of `docs/api/cli.md`.

#![cfg(test)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
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
        self.launch_with(args, stdin, no_herdr, &[])
    }

    /// `launch` with extra variables, such as a member's identity.
    fn launch_with(
        &self,
        args: &[&str],
        stdin: Option<&str>,
        no_herdr: bool,
        vars: &[(&str, &str)],
    ) -> Output {
        let mut cmd = command(env!("CARGO_BIN_EXE_teamwire"));
        cmd.arg("launch")
            .args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .envs(vars.iter().copied())
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
    assert!(err.contains("host none cannot start a member"), "{err}");
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
fn dry_run_shows_each_members_harness_model_effort_and_task_class() {
    let rig = Rig::new();
    let mut spec = pane_spec("demo", rig.work.path());
    spec["children"][0]["model"] = json!("sonnet");
    spec["children"][0]["effort"] = json!("high");
    spec["children"][0]["task_class"] = json!("root_cause");
    spec["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role": "b", "title": "Beta", "mode": "pane",
                     "cwd": rig.work.path(), "task": "look around"}));
    let file = rig.spec_file(&spec);
    let out = rig.launch(
        &["--dry-run", "--spec", file.to_str().unwrap()],
        None,
        false,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(
        shown.contains("on harness claude-code with sonnet at high, task class root_cause"),
        "{shown}"
    );
    assert!(
        shown.contains(
            "on harness claude-code with the default model at the default effort, task class none"
        ),
        "{shown}"
    );
    let err = text(&out.stderr);
    assert!(
        err.contains(
            "warning: member b has no task_class; outcomes for it cannot be read per class"
        ),
        "{err}"
    );
    assert!(!err.contains("member a has no task_class"), "{err}");
}

#[test]
fn an_unknown_task_class_is_refused_and_nothing_is_created() {
    let rig = Rig::new();
    let mut spec = pane_spec("demo", rig.work.path());
    spec["children"][0]["task_class"] = json!("tricky");
    let file = rig.spec_file(&spec);
    let out = rig.launch(&["--spec", file.to_str().unwrap()], None, false);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(
        err.contains("refused [a]: task_class_unknown") && err.contains("mechanical_edit"),
        "{err}"
    );
    assert!(rig.home_is_empty());
}

#[test]
fn an_unknown_key_is_warned_about_and_the_launch_goes_ahead() {
    let rig = Rig::new();
    let mut spec = pane_spec("demo", rig.work.path());
    spec["titel"] = json!("typo");
    spec["children"][0]["acept_trust"] = json!(false);
    let file = rig.spec_file(&spec);
    let file = file.to_str().unwrap();
    for args in [&["--dry-run", "--spec", file][..], &["--spec", file][..]] {
        let out = rig.launch(args, None, false);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            text(&out.stderr)
        );
        let err = text(&out.stderr);
        assert!(
            err.contains(r#"warning: team level: unknown key "titel" is ignored"#)
                && err.contains(r#"warning: role a: unknown key "acept_trust" is ignored"#),
            "{args:?}: {err}"
        );
        let shown = text(&out.stdout);
        let real = !args.contains(&"--dry-run");
        assert_eq!(shown.contains("a: started"), real, "{args:?}: {shown}");
    }
}

#[test]
fn a_launch_that_fails_is_exit_2_with_the_member_failed_and_cleaned_up() {
    let mut rig = Rig::new();
    // Whatever the adapters are, starting the agent fails.
    rig.herdr
        // Not `agent_pane_busy`: the adapter retries that 8 times, 1.5 s apart.
        .set_faults(&HerdrFaults::new().error("agent start", None, None, "invalid_params"))
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
        shown.contains("no member started; fix the cause and run teamwire launch --add --spec <file>, or teamwire close demo"),
        "{shown}"
    );
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

/// Every file under `dir` whose name ends with `suffix`.
fn files_ending(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_ending(&path, suffix));
        } else if path.to_string_lossy().ends_with(suffix) {
            found.push(path);
        }
    }
    found
}

#[test]
fn a_relative_home_reaches_members_as_an_absolute_path() {
    let rig = Rig::new();
    let cwd = tempfile::tempdir().unwrap();
    let spec = pane_spec("rel", rig.work.path());
    let mut cmd = command(env!("CARGO_BIN_EXE_teamwire"));
    cmd.args(["--home", "rel-home", "launch"])
        .current_dir(cwd.path())
        .env_clear()
        .envs(rig.herdr.env())
        .env("HERDR_ENV", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(spec.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let home = cwd.path().join("rel-home");
    let settings = files_ending(&home, "settings.json");
    let mcp = files_ending(&home, "mcp.json");
    assert!(
        !settings.is_empty() && !mcp.is_empty(),
        "{settings:?} {mcp:?}"
    );
    for file in &settings {
        let value: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        let shown = value["env"]["TEAMWIRE_HOME"].as_str().unwrap_or_default();
        assert!(Path::new(shown).is_absolute(), "{file:?}: {shown:?}");
    }
    for file in &mcp {
        let value: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        let args = value.to_string();
        assert!(!args.contains("\"rel-home\""), "{file:?}: {args}");
        assert!(
            args.contains(
                &cwd.path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            ),
            "{args}"
        );
    }
}

#[test]
fn a_member_whose_harness_program_is_missing_is_refused_before_anything_is_created() {
    let rig = Rig::new();
    let spec = json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "cx", "title": "Codex work", "mode": "pane", "cwd": rig.work.path(),
         "task": "look around", "harness": "codex"}]});
    let file = rig.spec_file(&spec);
    for extra in [&["--dry-run"][..], &[][..]] {
        let mut args = extra.to_vec();
        args.extend(["--spec", file.to_str().unwrap()]);
        let out = rig.launch(&args, None, false);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{extra:?}: {}",
            text(&out.stdout)
        );
        let err = text(&out.stderr);
        assert!(
            err.contains("refused: member cx: harness codex is not on PATH"),
            "{err}"
        );
        assert!(!text(&out.stdout).contains("would launch"));
        assert!(rig.home_is_empty(), "{extra:?}: no team directory");
    }
}

#[test]
fn a_pi_member_launches_when_pi_is_on_path() {
    let rig = Rig::new();
    let spec = json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "scout", "title": "pi work", "mode": "pane", "cwd": rig.work.path(),
         "task": "look around", "harness": "pi"}]});
    let file = rig.spec_file(&spec);
    let args = ["--dry-run", "--spec", file.to_str().unwrap()];
    let out = rig.launch(&args, None, false);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stdout));
    assert!(
        text(&out.stderr).contains("refused: member scout: harness pi is not on PATH"),
        "{}",
        text(&out.stderr)
    );

    let stubs = rig.work.path().join("stubs");
    std::fs::create_dir_all(&stubs).unwrap();
    let pi = stubs.join("pi");
    write_file(&pi, "#!/bin/sh\necho 1.4.0\n");
    std::fs::set_permissions(&pi, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let fakes = rig
        .herdr
        .env()
        .into_iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v)
        .unwrap();
    let path = format!("{}:{fakes}", stubs.display());
    let out = rig.launch_with(&args, None, false, &[("PATH", path.as_str())]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("would launch"),
        "{}",
        text(&out.stdout)
    );
}

#[test]
fn the_unverifiable_implementer_warning_names_what_is_missing() {
    let rig = Rig::new();
    let repo = rig.repo();
    let child = |role: &str, extra: Value| {
        let mut c = json!({"role": role, "title": format!("Work {role}"), "kind": "implementer",
            "mode": "worktree", "repo": repo, "task": "build"});
        c.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        c
    };
    let spec = json!({"team": "demo", "title": "Demo team", "children": [
        child("nocheck", json!({"owns": ["a/"]})),
        child("noowns", json!({"check": "true"})),
        child("neither", json!({}))]});
    let file = rig.spec_file(&spec);
    let out = rig.launch(
        &["--dry-run", "--spec", file.to_str().unwrap()],
        None,
        false,
    );
    let err = text(&out.stderr);
    for line in [
        "role nocheck: an implementer without a check cannot be verified",
        "role noowns: an implementer without an owns list cannot be verified",
        "role neither: an implementer without a check and an owns list cannot be verified",
    ] {
        assert!(err.contains(line), "{err}");
    }
}

#[test]
fn the_empty_launch_line_names_the_spec_file_the_user_passed() {
    let mut rig = Rig::new();
    rig.herdr
        // Not `agent_pane_busy`: the adapter retries that 8 times, 1.5 s apart.
        .set_faults(&HerdrFaults::new().error("agent start", None, None, "invalid_params"))
        .unwrap();
    let file = rig.spec_file(&pane_spec("demo", rig.work.path()));
    let out = rig.launch(&["--spec", file.to_str().unwrap()], None, false);
    let shown = text(&out.stdout);
    let want = format!(
        "teamwire launch --add --spec {}, or teamwire close demo",
        file.display()
    );
    assert!(shown.contains(&want), "{shown}");
}

#[test]
fn a_bad_ref_is_refused_by_launch_and_by_dry_run() {
    let rig = Rig::new();
    let mut spec = pane_spec("demo", rig.work.path());
    spec["refs"] = json!([{"provider": "linear", "id": "GKA 580"}]);
    let file = rig.spec_file(&spec);
    for extra in [&["--dry-run"][..], &[][..]] {
        let mut args = extra.to_vec();
        args.extend(["--spec", file.to_str().unwrap()]);
        let out = rig.launch(&args, None, false);
        assert_eq!(out.status.code(), Some(1), "{extra:?}");
        assert!(
            text(&out.stderr).contains("bad_ref"),
            "{}",
            text(&out.stderr)
        );
        assert!(rig.home_is_empty(), "{extra:?}: nothing was created");
    }
}

#[test]
fn a_member_cannot_launch_a_team_but_may_ask_for_the_plan() {
    let rig = Rig::new();
    let file = rig.spec_file(&pane_spec("nested", rig.work.path()));
    let file = file.to_str().unwrap();
    let member: [&[(&str, &str)]; 3] = [
        &[("TEAMWIRE_TOKEN", "not-a-real-token")],
        &[("TEAMWIRE_TEAM", "alpha"), ("TEAMWIRE_ROLE", "api")],
        &[("TEAMWIRE_ROLE", "api")],
    ];
    for vars in member {
        for args in [&["--spec", file][..], &["--add", "--spec", file]] {
            let out = rig.launch_with(args, None, false, vars);
            let err = text(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{vars:?} {args:?}: {err}");
            assert!(err.contains("refused.identity"), "{err}");
            assert!(
                rig.home_is_empty(),
                "{vars:?} {args:?}: no team was created"
            );
        }
        // A dry run creates nothing, so a member may ask for the plan.
        let out = rig.launch_with(&["--dry-run", "--spec", file], None, false, vars);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{vars:?}: {}",
            text(&out.stderr)
        );
        assert!(rig.home_is_empty(), "{vars:?}: a dry run creates nothing");
        // With no running team `--add` is refused, but not for who asks.
        let out = rig.launch_with(&["--dry-run", "--add", "--spec", file], None, false, vars);
        let err = text(&out.stderr);
        assert!(
            err.contains("no_such_team") && !err.contains("refused.identity"),
            "{err}"
        );
        assert!(rig.home_is_empty(), "{vars:?}: a dry run creates nothing");
    }
}

#[test]
fn dry_run_names_the_refs_it_would_record_and_then_append() {
    let rig = Rig::new();
    let mut spec = pane_spec("demo", rig.work.path());
    spec["refs"] = json!([
        {"provider": "linear", "id": "GKA-580"},
        {"provider": "github", "id": "#1"},
        {"provider": "linear", "id": "GKA-580"},
    ]);
    let file = rig.spec_file(&spec);
    let dry = rig.launch(
        &["--dry-run", "--spec", file.to_str().unwrap()],
        None,
        false,
    );
    assert_eq!(dry.status.code(), Some(0), "{}", text(&dry.stderr));
    assert!(
        text(&dry.stdout).contains("would record refs linear:GKA-580, github:#1"),
        "{}",
        text(&dry.stdout)
    );
    assert!(rig.home_is_empty());

    let out = rig.launch(&["--spec", file.to_str().unwrap()], None, false);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let store = Store::open_read_only(rig.home.path(), &TeamName::parse("demo").unwrap()).unwrap();
    let refs = store.read(|tx| Ok(tx.team()?.map(|t| t.refs))).unwrap();
    let shown: Vec<String> = refs.unwrap().iter().map(ToString::to_string).collect();
    assert_eq!(shown, ["linear:GKA-580", "github:#1"]);
    drop(store);

    let mut more = pane_spec("demo", rig.work.path());
    more["children"][0]["role"] = json!("b");
    more["children"][0]["title"] = json!("Beta");
    more["refs"] = json!([
        {"provider": "github", "id": "#1"},
        {"provider": "github", "id": "#2"},
        {"provider": "gitlab", "id": "#1"},
    ]);
    let file = rig.spec_file(&more);
    let dry = rig.launch(
        &["--add", "--dry-run", "--spec", file.to_str().unwrap()],
        None,
        false,
    );
    assert_eq!(dry.status.code(), Some(0), "{}", text(&dry.stderr));
    assert!(
        text(&dry.stdout).contains("would append refs github:#2, gitlab:#1"),
        "{}",
        text(&dry.stdout)
    );
}

#[test]
fn a_bare_string_ref_is_refused_naming_the_object_form() {
    let rig = Rig::new();
    let mut spec = pane_spec("demo", rig.work.path());
    spec["refs"] = json!(["GKA-580"]);
    let out = rig.launch(&[], Some(&spec.to_string()), false);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(
        err.contains("bad_ref") && err.contains("\"provider\": \"linear\""),
        "{err}"
    );
    assert!(rig.home_is_empty());
}

#[test]
fn a_launch_that_fails_in_the_store_leaves_one_store_error_row() {
    let rig = Rig::new();
    let first = rig.spec_file(&pane_spec("demo", rig.work.path()));
    let out = rig.launch(&["--spec", first.to_str().unwrap()], None, false);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let team = TeamName::parse("demo").unwrap();
    let mut store = Store::open_existing(rig.home.path(), &team).unwrap();
    // Added after the plan was checked: a function that does not exist fails
    // the new member's insert with an `io` error.
    store
        .write(|tx| {
            tx.connection()
                .execute_batch(
                    "CREATE TRIGGER no_member BEFORE INSERT ON members
                     BEGIN SELECT no_such_function(); END",
                )
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let more = json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "b", "title": "Beta", "mode": "pane", "cwd": rig.work.path(), "task": "look around"}]});
    let file = rig.spec_file(&more);
    let out = rig.launch(&["--add", "--spec", file.to_str().unwrap()], None, false);
    assert_ne!(out.status.code(), Some(0), "{}", text(&out.stdout));
    let errors: Vec<Value> = store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == teamwire_core::EventType::StoreError)
        .map(|e| e.data)
        .collect();
    assert_eq!(
        errors,
        [json!({ "op": "launch", "code": "io" })],
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}
