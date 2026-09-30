//! The folder-trust step: the saved-trust read, and the dialog answers,
//! against `FakeHost` screens and against a real `fake-claude` process.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Harness, HarnessErrorCode, Host, HostCaps, HostError, HostId, Key, NoHost, Occupant, OneLine,
    Pid, Place, PlaceState, Platform, SpawnRequest, StartupOutcome, TrustPolicy,
};
use teamwire_fakes::{
    ClaudeScript, ESCALATED_TRUST_TEXT, FakeHerdrEnv, FakeHost, FakePlatform, PLAIN_TRUST_TEXT,
    Startup, TRUST_ESCALATION_MARKERS, TRUST_OPTION_TEXT,
};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig, StartupPoll, TrustCheck};

fn fast() -> StartupPoll {
    StartupPoll {
        step: Duration::from_millis(1),
        steps: 3,
    }
}

fn claude_in(dir: &Path) -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: dir.join(".claude"),
            claude_json: dir.join(".claude.json"),
            claude_bin: "claude".into(),
        },
    )
    .with_startup_poll(fast())
}

fn host_with_screen(screen: &str) -> (FakeHost, Place) {
    let host = FakeHost::new();
    let place = host.add_place(
        "tw_demo_api",
        PlaceState::Blocked,
        Occupant {
            session: None,
            pids: vec![Pid::new(1)],
        },
    );
    host.set_screen(&place, screen);
    (host, place)
}

const PLAIN_DIALOG: &str = "Accessing workspace:\n /work\n\n Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your team).\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel";

fn answer(
    screen: &str,
    policy: TrustPolicy,
) -> (
    Result<StartupOutcome, teamwire_core::HarnessError>,
    Vec<Key>,
) {
    let dir = tempfile::tempdir().unwrap();
    let (host, place) = host_with_screen(screen);
    let got = claude_in(dir.path()).answer_startup(&host, &place, policy);
    (got, host.keys_sent(&place))
}

#[test]
fn escalated_trust_refused() {
    let (got, keys) = answer(
        &format!("{PLAIN_TRUST_TEXT}\n{TRUST_OPTION_TEXT}\n{ESCALATED_TRUST_TEXT}"),
        TrustPolicy::Accept,
    );
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty(), "nothing is typed into an escalated dialog");
}

#[test]
fn each_escalation_marker_alone_refuses() {
    for marker in TRUST_ESCALATION_MARKERS {
        let screen = format!("{PLAIN_DIALOG}\nThis folder: {marker} something");
        let (got, keys) = answer(&screen, TrustPolicy::Accept);
        assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated, "{marker}");
        assert!(keys.is_empty(), "{marker}");
    }
}

#[test]
fn an_escalation_marker_wrapped_across_lines_still_refuses() {
    let screen = format!("{PLAIN_DIALOG}\nThese will apply\nwithout asking.");
    let (got, keys) = answer(&screen, TrustPolicy::Accept);
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty());
}

#[test]
fn escalated_dialog_is_refused_even_when_the_policy_refuses() {
    let (got, keys) = answer(ESCALATED_TRUST_TEXT, TrustPolicy::Refuse);
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty());
}

#[test]
fn the_refuse_policy_answers_nothing_on_a_plain_dialog() {
    let (got, keys) = answer(PLAIN_DIALOG, TrustPolicy::Refuse);
    assert_eq!(got.unwrap_err().code, HarnessErrorCode::Refused);
    assert!(keys.is_empty());
}

#[test]
fn each_plain_marker_alone_is_a_plain_dialog() {
    for marker in [TRUST_OPTION_TEXT, "Is this a project you created"] {
        let (got, keys) = answer(marker, TrustPolicy::Accept);
        // The static screen never shows the prompt afterwards, so the
        // confirmation times out; the keys still prove the answer.
        assert_eq!(got.unwrap_err().code, HarnessErrorCode::Timeout, "{marker}");
        assert_eq!(keys, [Key::Down, Key::Enter], "{marker}");
    }
}

#[test]
fn the_answer_is_down_then_enter_and_never_a_digit() {
    let (_, keys) = answer(PLAIN_DIALOG, TrustPolicy::Accept);
    assert_eq!(keys, [Key::Down, Key::Enter]);
}

#[test]
fn an_input_prompt_means_no_dialog() {
    let (got, keys) = answer(
        "welcome\n╭───╮\n│ ❯ │\n╰───╯\n  ? for shortcuts",
        TrustPolicy::Accept,
    );
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
    assert!(keys.is_empty());
}

#[test]
fn a_screen_with_neither_prompt_nor_dialog_times_out_and_types_nothing() {
    let (got, keys) = answer("starting up...", TrustPolicy::Accept);
    assert_eq!(got.unwrap_err().code, HarnessErrorCode::Timeout);
    assert!(keys.is_empty());
}

#[test]
fn a_host_failure_is_reported_as_a_host_error() {
    let dir = tempfile::tempdir().unwrap();
    let host = FakeHost::new();
    let place = Place {
        host: HostId::from_static("fake"),
        id: "nope".into(),
    };
    let err = claude_in(dir.path())
        .answer_startup(&host, &place, TrustPolicy::Accept)
        .unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Host);
}

// ---- the saved-trust read -------------------------------------------------

struct World {
    dir: tempfile::TempDir,
    claude: ClaudeCode,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        let claude = ClaudeCode::new(
            Arc::new(FakePlatform::new()),
            ClaudeConfig {
                claude_dir: dir.path().join("home/.claude"),
                claude_json: dir.path().join("home/.claude.json"),
                claude_bin: "claude".into(),
            },
        );
        Self { dir, claude }
    }

    fn path(&self, rel: &str) -> std::path::PathBuf {
        let p = self.dir.path().join(rel);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::canonicalize(p).unwrap()
    }

    fn trust(&self, entries: &[(&Path, bool)]) {
        let projects: serde_json::Map<_, _> = entries
            .iter()
            .map(|(p, ok)| {
                (
                    p.display().to_string(),
                    json!({ "hasTrustDialogAccepted": ok }),
                )
            })
            .collect();
        FakePlatform::new()
            .write_atomic(
                &self.dir.path().join("home/.claude.json"),
                json!({ "projects": projects }).to_string().as_bytes(),
            )
            .unwrap();
    }
}

#[test]
fn no_config_file_means_trust_is_missing() {
    let w = World::new();
    assert_eq!(
        w.claude.trust_check(&w.path("work/proj")),
        TrustCheck::Missing
    );
}

#[test]
fn saved_trust_for_the_directory_is_persisted() {
    let w = World::new();
    let proj = w.path("work/proj");
    w.trust(&[(&proj, true)]);
    assert_eq!(w.claude.trust_check(&proj), TrustCheck::Persisted);
}

#[test]
fn an_entry_that_is_not_accepted_is_missing() {
    let w = World::new();
    let proj = w.path("work/proj");
    w.trust(&[(&proj, false)]);
    assert_eq!(w.claude.trust_check(&proj), TrustCheck::Missing);
}

#[test]
fn a_plain_directory_inherits_trust_from_a_parent() {
    let w = World::new();
    let work = w.path("work");
    let sub = w.path("work/plain/sub");
    w.trust(&[(&work, true)]);
    assert_eq!(w.claude.trust_check(&sub), TrustCheck::Persisted);
}

#[test]
fn a_git_repository_is_a_trust_boundary() {
    let w = World::new();
    let work = w.path("work");
    let repo = w.path("work/repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    w.trust(&[(&work, true)]);
    assert_eq!(w.claude.trust_check(&repo), TrustCheck::Missing);
    let deep = w.path("work/repo/src");
    assert_eq!(w.claude.trust_check(&deep), TrustCheck::Missing);
    w.trust(&[(&repo, true)]);
    assert_eq!(w.claude.trust_check(&deep), TrustCheck::Persisted);
}

#[test]
fn a_linked_worktree_is_covered_by_its_main_checkout() {
    let w = World::new();
    let main = w.path("work/main");
    std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
    let wt = w.path("work/wt");
    FakePlatform::new()
        .write_atomic(
            &wt.join(".git"),
            format!("gitdir: {}/.git/worktrees/wt\n", main.display()).as_bytes(),
        )
        .unwrap();
    assert_eq!(w.claude.trust_check(&wt), TrustCheck::Missing);
    w.trust(&[(&main, true)]);
    assert_eq!(w.claude.trust_check(&wt), TrustCheck::Persisted);
}

#[test]
fn the_home_directory_is_never_persisted() {
    let w = World::new();
    let home = w.path("home");
    w.trust(&[(&home, true)]);
    assert_eq!(w.claude.trust_check(&home), TrustCheck::Home);
}

// ---- a real fake-claude process ---------------------------------------------

/// A host that drives one agent through `fake-herdr`'s command line.
struct HerdrCli<'a> {
    env: &'a FakeHerdrEnv,
}

impl HerdrCli<'_> {
    fn unsupported() -> HostError {
        NoHost
            .close(
                &Place {
                    host: HostId::from_static("none"),
                    id: String::new(),
                },
                Duration::ZERO,
            )
            .unwrap_err()
    }
}

impl Host for HerdrCli<'_> {
    fn id(&self) -> HostId {
        HostId::from_static("herdr")
    }
    fn capabilities(&self) -> HostCaps {
        NoHost.capabilities()
    }
    fn spawn(&self, _r: &SpawnRequest, _t: Duration) -> Result<Place, HostError> {
        Err(Self::unsupported())
    }
    fn submit(&self, _p: &Place, _x: &OneLine, _t: Duration) -> Result<(), HostError> {
        Err(Self::unsupported())
    }
    fn state(&self, _p: &Place, _t: Duration) -> Result<PlaceState, HostError> {
        Err(Self::unsupported())
    }
    fn occupant(&self, _p: &Place, _t: Duration) -> Result<Occupant, HostError> {
        Err(Self::unsupported())
    }
    fn read_tail(&self, _p: &Place, lines: u16, _t: Duration) -> Result<String, HostError> {
        let out = self
            .env
            .run(&[
                "agent",
                "read",
                "tw_demo_api",
                "--lines",
                &lines.to_string(),
            ])
            .map_err(|_| Self::unsupported())?;
        Ok(out.stdout)
    }
    fn send_keys(&self, _p: &Place, keys: &[Key], _t: Duration) -> Result<(), HostError> {
        for key in keys {
            let name = match key {
                Key::Down => "down",
                Key::Enter => "enter",
                Key::Char(_)
                | Key::Up
                | Key::Left
                | Key::Right
                | Key::Tab
                | Key::Escape
                | Key::Backspace
                | Key::CtrlC => "esc",
            };
            self.env
                .run(&["agent", "send-keys", "tw_demo_api", name])
                .map_err(|_| Self::unsupported())?;
        }
        Ok(())
    }
    fn close(&self, _p: &Place, _t: Duration) -> Result<(), HostError> {
        Err(Self::unsupported())
    }
    fn list(&self, _t: Duration) -> Result<Vec<Place>, HostError> {
        Ok(Vec::new())
    }
    fn live_agent_names(&self, _t: Duration) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }
    fn place_env_var(&self) -> Option<&'static str> {
        None
    }
}

fn started(startup: Startup) -> FakeHerdrEnv {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        startup,
        ..ClaudeScript::default()
    })
    .unwrap();
    let pane = env
        .run(&["workspace", "create", "--cwd", "/tmp", "--no-focus"])
        .unwrap()
        .result()
        .unwrap()
        .pointer("/root_pane/pane_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    // With a dialog open the start reports not-ready and leaves the agent running.
    env.run(&[
        "agent",
        "start",
        "tw_demo_api",
        "--kind",
        "claude",
        "--pane",
        &pane,
        "--timeout",
        "5000",
        "--",
        "-n",
        "tw_demo_api",
        "--session-id",
        "s-trust",
    ])
    .unwrap();
    env
}

fn real_answer(
    env: &FakeHerdrEnv,
    policy: TrustPolicy,
) -> Result<StartupOutcome, teamwire_core::HarnessError> {
    let dir = tempfile::tempdir().unwrap();
    let claude = claude_in(dir.path()).with_startup_poll(StartupPoll {
        step: Duration::from_millis(100),
        steps: 50,
    });
    let place = Place {
        host: HostId::from_static("herdr"),
        id: "p".into(),
    };
    claude.answer_startup(&HerdrCli { env }, &place, policy)
}

fn agent_status(env: &FakeHerdrEnv) -> String {
    env.run(&["agent", "get", "tw_demo_api"])
        .unwrap()
        .result()
        .unwrap()
        .pointer("/agent/agent_status")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn fake_claude_plain_dialog_is_answered_and_the_prompt_appears() {
    let env = started(Startup::Trust);
    assert_eq!(agent_status(&env), "blocked");
    assert_eq!(
        real_answer(&env, TrustPolicy::Accept).unwrap(),
        StartupOutcome::AnsweredPlainTrust
    );
    assert_ne!(agent_status(&env), "blocked");
}

#[test]
fn fake_claude_escalated_dialog_is_left_alone() {
    let env = started(Startup::EscalatedTrust);
    assert_eq!(
        real_answer(&env, TrustPolicy::Accept).unwrap(),
        StartupOutcome::RefusedEscalated
    );
    assert_eq!(agent_status(&env), "blocked");
}

#[test]
fn fake_claude_without_a_dialog_is_no_dialog() {
    let env = started(Startup::None);
    assert_eq!(
        real_answer(&env, TrustPolicy::Accept).unwrap(),
        StartupOutcome::NoDialog
    );
}
