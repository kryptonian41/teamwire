//! `teamwire close` through the real binary: teams launched by the real
//! `teamwire launch` against `fake-herdr` and `fake-claude`, in a temporary
//! home and a throwaway repository. Exit codes are those of `docs/api/cli.md`.
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and git in a throwaway repository"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::watch_lease` as a path does not infer the transaction's lifetime for `Store::read`"
)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};
use teamwire_core::{TeamName, TeamState};
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

impl Rig {
    fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    /// Runs `teamwire <args>` with a cleared environment: the fakes' `PATH`, a
    /// temporary home, and `HERDR_ENV=1`.
    fn teamwire(&self, args: &[&str], stdin: Option<&str>) -> Run {
        self.teamwire_with(args, stdin, &[])
    }

    /// `teamwire` with extra variables, such as a member's identity.
    fn teamwire_with(&self, args: &[&str], stdin: Option<&str>, extra: &[(&str, &str)]) -> Run {
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
        let Output {
            status,
            stdout,
            stderr,
        } = child.wait_with_output().unwrap();
        Run {
            code: status.code().unwrap(),
            out: String::from_utf8_lossy(&stdout).into_owned(),
            err: String::from_utf8_lossy(&stderr).into_owned(),
        }
    }

    fn launch(&self, spec: &Value) -> Run {
        let run = self.teamwire(&["launch"], Some(&spec.to_string()));
        assert_eq!(run.code, 0, "launch: {}{}", run.out, run.err);
        self.wait_for_first_turns();
        run
    }

    fn team_dir(&self, team: &str) -> PathBuf {
        self.home.path().join("teams").join(team)
    }
}

fn pane_spec(work: &Path) -> Value {
    json!({"team": "alpha", "title": "Alpha", "children": [
        {"role": "scout", "title": "Scout", "mode": "pane", "cwd": work, "task": "look around"}]})
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
    fn repo(&self) -> PathBuf {
        let repo = self.work.path().join("proj");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join(".gitignore"), ".worktrees/\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "first"]);
        repo
    }

    fn store(&self) -> Store {
        Store::open_read_only(self.home.path(), &TeamName::parse("alpha").unwrap()).unwrap()
    }

    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    fn team_state(&self) -> TeamState {
        self.store().read(|tx| tx.team()).unwrap().unwrap().state
    }

    /// The file through which `fake-herdr` reads a pane's agent status.
    fn status_file(&self, place_id: &str) -> PathBuf {
        self.herdr
            .root()
            .join("fake-herdr-panes")
            .join(place_id.replace(':', "_"))
            .join("status.json")
    }

    /// Waits until every member has ended the turn its launch task started.
    /// `close` reads busy-ness from that status, and `fake-claude` writes
    /// `done` only after its Stop hooks return. A hook can take any time to
    /// start, so a pause in the status says nothing; only `done` does.
    fn wait_for_first_turns(&self) {
        let members = self.store().read(|tx| tx.members()).unwrap().items;
        for member in members {
            let Some(place) = member.place else { continue };
            let file = self.status_file(&place.id);
            let done = || {
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                serde_json::from_str::<Value>(&text)
                    .is_ok_and(|s| s.get("status").is_some_and(|v| v == "done"))
            };
            // 30 seconds, counted in polls so the test needs no clock.
            let ended = (0..1500).any(|_| {
                let ended = done();
                if !ended {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                ended
            });
            assert!(
                ended,
                "{}'s first turn did not end: {}",
                member.role.as_str(),
                std::fs::read_to_string(&file).unwrap_or_default()
            );
        }
    }

    /// Makes the member's pane report `status` to `fake-herdr`. `launch`
    /// has waited out the first turn, so `fake-claude` writes no more.
    fn set_status(&self, role: &str, status: &str) {
        let member = self
            .store()
            .read(|tx| tx.member(&teamwire_core::Role::parse(role).unwrap()))
            .unwrap()
            .unwrap();
        std::fs::write(
            self.status_file(&member.place.unwrap().id),
            format!(r#"{{"status":"{status}","seq":99}}"#),
        )
        .unwrap();
    }
}

fn api_spec(repo: &Path) -> Value {
    json!({"team": "alpha", "title": "Alpha", "children": [
        {"role": "api", "title": "API work", "mode": "worktree", "repo": repo,
         "task": "build", "check": "true", "owns": ["src/"]},
        {"role": "scout", "title": "Scout", "mode": "pane", "cwd": repo, "task": "look around"}]})
}

/// `SHA256SUMS` is the standard format, so the system's own tool checks it.
fn assert_archive_checks_out(archive: &Path) {
    let sums = std::fs::read_to_string(archive.join("SHA256SUMS")).unwrap();
    assert!(sums.lines().any(|l| l.ends_with("  team.db")), "{sums}");
    let out = std::process::Command::new("shasum")
        .args(["-a", "256", "-c", "SHA256SUMS"])
        .current_dir(archive)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn archive_named(run: &Run) -> PathBuf {
    assert!(
        run.out.contains("closed team"),
        "no closing line: {}{}",
        run.out,
        run.err
    );
    let line = run
        .out
        .lines()
        .find(|l| l.starts_with("closed team"))
        .unwrap();
    PathBuf::from(line.rsplit("Archive: ").next().unwrap())
}

#[test]
fn dry_run_prints_the_plan_and_changes_nothing() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    let db = std::fs::read(rig.team_dir("alpha").join("team.db")).unwrap();
    let run = rig.teamwire(&["close", "alpha", "--dry-run"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for step in [
        "check",
        "lock",
        "archive",
        "remove_worktrees",
        "close_places",
        "release_hold",
        "finish",
        "delete_state",
    ] {
        assert!(
            run.out.contains(&format!("[dry-run] {step}:")),
            "{step}: {}",
            run.out
        );
    }
    assert_eq!(rig.team_state(), TeamState::Open);
    assert!(!rig.home.path().join("archive").exists());
    assert_eq!(
        std::fs::read(rig.team_dir("alpha").join("team.db")).unwrap(),
        db
    );
    // The place is still open: a real close works afterwards.
    let close = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(close.code, 0, "{}{}", close.out, close.err);
}

#[test]
fn a_full_close_exits_zero_archives_and_names_the_branches_and_unpushed_work() {
    let rig = Rig::new();
    let repo = rig.repo();
    rig.launch(&api_spec(&repo));
    let worktree = repo.join(".worktrees/api");
    assert!(worktree.is_dir());
    std::fs::write(worktree.join("lib.rs"), "fn a() {}\n").unwrap();
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-q", "-m", "work"]);

    let run = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(!worktree.exists(), "the worktree was removed");
    let branches = git(&repo, &["branch", "--list"]);
    assert!(
        branches.contains("main") && branches.lines().count() >= 2,
        "the branch is kept: {branches}"
    );
    assert!(run.out.contains("branches kept:"), "{}", run.out);
    assert!(
        run.out.contains("unpushed:") && run.out.contains("1 commit(s)"),
        "{}",
        run.out
    );
    assert!(!rig.team_dir("alpha").exists());
    let archive = archive_named(&run);
    assert!(archive.starts_with(rig.home.path().join("archive")));
    assert_archive_checks_out(&archive);
    let steps: Vec<&str> = run
        .out
        .lines()
        .filter_map(|l| l.split(':').next())
        .take(9)
        .collect();
    assert_eq!(
        steps,
        [
            "lock",
            "archive",
            "verify_archive",
            "remove_worktrees",
            "close_places",
            "release_hold",
            "finish",
            "archive_final",
            "delete_state"
        ]
    );

    // There is nothing left to close.
    let again = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(again.code, 1, "{}{}", again.out, again.err);
    assert!(again.err.contains("no database"), "{}", again.err);
}

#[test]
fn a_busy_member_is_exit_one_and_nothing_changes() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    rig.set_status("scout", "working");
    let run = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("REFUSED: scout is busy"), "{}", run.err);
    assert!(run.err.contains("nothing was changed"), "{}", run.err);
    assert_eq!(rig.team_state(), TeamState::Open);
    assert!(rig.team_dir("alpha").join("team.db").is_file());
    assert!(!rig.home.path().join("archive").exists());

    // The dry run says the same, with the same exit code.
    let dry = rig.teamwire(&["close", "alpha", "--dry-run"], None);
    assert_eq!(dry.code, 1, "{}{}", dry.out, dry.err);
    assert!(dry.err.contains("REFUSED: scout is busy"), "{}", dry.err);

    rig.set_status("scout", "idle");
    let run = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
}

#[test]
fn uncommitted_work_is_exit_one() {
    let rig = Rig::new();
    let repo = rig.repo();
    rig.launch(&api_spec(&repo));
    let worktree = repo.join(".worktrees/api");
    std::fs::write(worktree.join("wip.txt"), "not committed\n").unwrap();
    let run = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(
        run.err.contains("api has 1 uncommitted path"),
        "{}",
        run.err
    );
    assert!(worktree.join("wip.txt").is_file());
    assert_eq!(rig.team_state(), TeamState::Open);
}

#[test]
fn a_step_that_fails_is_exit_two_with_the_state_intact_and_a_rerun_finishes() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    // The archive's parent is a file, so the archive cannot be made.
    let blocker = rig.work.path().join("blocker");
    std::fs::write(&blocker, "a file\n").unwrap();
    let inside = blocker.join("archives");
    let run = rig.teamwire(
        &["close", "alpha", "--archive-to", inside.to_str().unwrap()],
        None,
    );
    assert_eq!(run.code, 2, "{}{}", run.out, run.err);
    assert!(run.err.contains("PROBLEM archive:"), "{}", run.err);
    assert!(run.err.contains("close stopped at archive"), "{}", run.err);
    assert_eq!(rig.team_state(), TeamState::Open);
    assert!(rig.team_dir("alpha").join("team.db").is_file());

    let rerun = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(rerun.code, 0, "{}{}", rerun.out, rerun.err);
}

#[test]
fn keep_state_leaves_the_directory_and_archive_to_names_the_parent() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    let elsewhere = rig.work.path().join("out");
    let run = rig.teamwire(
        &[
            "close",
            "alpha",
            "--keep-state",
            "--archive-to",
            elsewhere.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(rig.team_dir("alpha").join("team.db").is_file());
    assert_eq!(rig.team_state(), TeamState::Closed);
    let archive = archive_named(&run);
    assert!(archive.starts_with(&elsewhere), "{}", archive.display());
    assert_archive_checks_out(&archive);
    assert!(!rig.home.path().join("archive").exists());
}

#[test]
fn clear_stale_lock_clears_a_dead_owner_and_only_that() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    // A close that crashed: the lock names a pid that is not running.
    let mut store =
        Store::open_existing(rig.home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
    let now = teamwire_core::Timestamp::parse("2026-09-29T10:00:00.000Z").unwrap();
    store
        .write(|tx| tx.begin_close(now, teamwire_core::Pid::new(2_000_000_000)))
        .unwrap();
    drop(store);

    let refused = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(refused.code, 1, "{}{}", refused.out, refused.err);
    assert!(
        refused.err.contains("--clear-stale-lock"),
        "{}",
        refused.err
    );

    let cleared = rig.teamwire(&["close", "alpha", "--clear-stale-lock"], None);
    assert_eq!(cleared.code, 0, "{}{}", cleared.out, cleared.err);
    assert!(cleared.out.contains("clear_stale_lock:"), "{}", cleared.out);
    assert_eq!(rig.team_state(), TeamState::Open);
    assert!(
        !rig.home.path().join("archive").exists(),
        "nothing else was done"
    );

    let run = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
}

#[test]
fn removing_a_member_needs_a_verify_record_unless_told_otherwise() {
    let rig = Rig::new();
    let repo = rig.repo();
    rig.launch(&api_spec(&repo));
    let refused = rig.teamwire(&["close", "alpha", "--role", "api"], None);
    assert_eq!(refused.code, 1, "{}{}", refused.out, refused.err);
    assert!(
        refused.err.contains("api has no verify record"),
        "{}",
        refused.err
    );
    assert!(repo.join(".worktrees/api").is_dir());

    let dry = rig.teamwire(
        &[
            "close",
            "alpha",
            "--role",
            "api",
            "--dry-run",
            "--unverified",
        ],
        None,
    );
    assert_eq!(dry.code, 0, "{}{}", dry.out, dry.err);
    assert!(
        dry.out
            .contains("[dry-run] unverified: removing anyway (--unverified)"),
        "{}",
        dry.out
    );
    assert!(
        repo.join(".worktrees/api").is_dir(),
        "a dry run removes nothing"
    );

    let run = rig.teamwire(&["close", "alpha", "--role", "api", "--unverified"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out
            .contains("unverified: removing anyway (--unverified): api has no verify record"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains("removed api from team alpha"),
        "{}",
        run.out
    );
    assert!(!repo.join(".worktrees/api").exists());
    assert!(run.out.contains("branches kept:"), "{}", run.out);
    // The rest of the team is untouched, and the team stays open.
    assert_eq!(rig.team_state(), TeamState::Open);
    let scout = rig.teamwire(&["close", "alpha", "--role", "scout"], None);
    assert_eq!(scout.code, 0, "{}{}", scout.out, scout.err);
}

#[test]
fn stop_is_the_only_way_to_remove_a_busy_member() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    rig.set_status("scout", "working");
    let refused = rig.teamwire(&["close", "alpha", "--role", "scout"], None);
    assert_eq!(refused.code, 1, "{}{}", refused.out, refused.err);
    assert!(
        refused.err.contains("--role scout --stop"),
        "{}",
        refused.err
    );
    let run = rig.teamwire(&["close", "alpha", "--role", "scout", "--stop"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.contains("that turn writes no report"),
        "{}",
        run.out
    );
}

#[test]
fn a_store_with_no_team_row_is_deleted_and_the_dry_run_changes_nothing() {
    let rig = Rig::new();
    let team = TeamName::parse("alpha").unwrap();
    drop(Store::open(rig.home.path(), &team).unwrap());
    let dir = rig.team_dir("alpha");
    let line = format!("alpha: nothing was recorded; deleted {}", dir.display());

    let dry = rig.teamwire(&["close", "alpha", "--dry-run"], None);
    assert_eq!(dry.code, 0, "{}{}", dry.out, dry.err);
    assert!(
        dry.out.contains(&format!("[dry-run] {line}")),
        "{}",
        dry.out
    );
    assert!(dir.join("team.db").is_file());

    let run = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.lines().any(|l| l == line), "{}", run.out);
    assert!(!dir.exists());
}

#[test]
fn usage_and_missing_state_have_their_own_exit_codes() {
    let rig = Rig::new();
    let bad_name = rig.teamwire(&["close", "my-team"], None);
    assert_eq!(bad_name.code, 3, "{}{}", bad_name.out, bad_name.err);
    let bad_role = rig.teamwire(&["close", "alpha", "--role", "Bad-Role"], None);
    assert_eq!(bad_role.code, 3, "{}{}", bad_role.out, bad_role.err);
    let missing = rig.teamwire(&["close", "alpha"], None);
    assert_eq!(missing.code, 1, "{}{}", missing.out, missing.err);
    assert!(
        missing.err.contains("no database for team alpha"),
        "{}",
        missing.err
    );

    rig.launch(&pane_spec(rig.work.path()));
    let nobody = rig.teamwire(&["close", "alpha", "--role", "nobody"], None);
    assert_eq!(nobody.code, 1, "{}{}", nobody.out, nobody.err);
    assert!(
        nobody.err.contains("no member nobody in team alpha"),
        "{}",
        nobody.err
    );
}

#[test]
fn a_member_cannot_remove_a_peer() {
    let rig = Rig::new();
    let repo = rig.repo();
    rig.launch(&api_spec(&repo));
    let member = [("TEAMWIRE_TEAM", "alpha"), ("TEAMWIRE_ROLE", "api")];
    let run = rig.teamwire_with(
        &[
            "close",
            "alpha",
            "--role",
            "scout",
            "--stop",
            "--unverified",
        ],
        None,
        &member,
    );
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("refused.identity"), "{}", run.err);
    let state = rig
        .store()
        .read(|tx| tx.member(&teamwire_core::Role::parse("scout").unwrap()))
        .unwrap()
        .unwrap()
        .state;
    assert_eq!(state, teamwire_core::MemberState::Started);
    // A dry run reads only, so a member may ask for the plan.
    let dry = rig.teamwire_with(&["close", "alpha", "--dry-run"], None, &member);
    assert_eq!(dry.code, 0, "{}{}", dry.out, dry.err);
}

#[test]
fn a_member_cannot_watch_or_add_members() {
    let rig = Rig::new();
    rig.launch(&pane_spec(rig.work.path()));
    let token = [("TEAMWIRE_TOKEN", "not-a-real-token")];
    for args in [&["watch", "alpha"][..], &["watch", "alpha", "--once"]] {
        let watch = rig.teamwire_with(args, None, &token);
        assert_eq!(watch.code, 1, "{args:?}: {}{}", watch.out, watch.err);
        assert!(watch.err.contains("refused.identity"), "{}", watch.err);
    }
    let add = rig.teamwire_with(&["launch", "--add"], Some("{}"), &token);
    assert_eq!(add.code, 1, "{}{}", add.out, add.err);
    assert!(add.err.contains("refused.identity"), "{}", add.err);
    // The refused watch took no lease.
    let lease = rig.store().read(|tx| tx.watch_lease()).unwrap();
    assert!(lease.is_none());
}
