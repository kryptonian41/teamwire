//! `answer_startup` against `FakeHost` screens. A helper thread plays the
//! TUI: after a number of keys it swaps the screen, as the real dialog
//! would redraw.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use teamwire_core::{
    Harness, HarnessError, HarnessErrorCode, Key, Occupant, Pid, Place, PlaceState, StartupOutcome,
    TrustPolicy,
};
use teamwire_fakes::{FakeHost, FakePlatform};
use teamwire_harness_codex::{CodexCli, CodexConfig, StartupPoll};

const IDLE: &str = "\n› Ask Codex to do anything\n\n  ? for shortcuts            100% context left";

// onboarding/snapshots/codex_tui__onboarding__trust_directory__tests__renders_snapshot_for_git_repo.snap
const TRUST_TEXT: &str = "
  Folder access
  /workspace/project

  Trust this folder? Codex can read, edit, and run files here,
  subject to your permission settings. Folder settings can run code
  automatically, even without a model request. Continue only if you
  trust these files. Your trust decision will be saved.
";
const TRUST: &str =
    "› 1. Trust and continue\n  2. Quit\n\n  enter continue and create sandbox · esc quit";
const TRUST_ON_QUIT: &str =
    "  1. Trust and continue\n› 2. Quit\n\n  enter continue and create sandbox · esc quit";

// snapshots/codex_tui__startup_hooks_review__tests__startup_hooks_review_prompt.snap
const HOOKS_REVIEW: &str = "
  Hooks need review
  2 hooks are new or changed.

› 1. Review hooks
  2. Trust all and continue
  3. Continue without trusting (hooks
     won't run)

  enter confirm · esc skip";

// snapshots/codex_tui__update_prompt__tests__update_prompt_modal.snap
const UPDATE_HEAD: &str = "
  Update available · 0.0.0 → 9.9.9
  Release notes: https://github.com/openai/codex/releases/latest
";
const UPDATE_1: &str = "› 1. Update now (runs `npm install -g @openai/codex`)\n  2. Skip\n  3. Skip until next version";
const UPDATE_2: &str = "  1. Update now (runs `npm install -g @openai/codex`)\n› 2. Skip\n  3. Skip until next version";

// snapshots/codex_tui__model_migration__tests__model_migration_prompt.snap
const MIGRATION_HEAD: &str = "
  Codex just got an upgrade. Introducing
  gpt-5.1-codex-max.
";
const MIGRATION_1: &str =
    "› 1. Try new model\n  2. Use existing model\n\n  enter/esc confirm · ⌃c quit";
// snapshots/codex_tui__cwd_prompt__tests__cwd_prompt_modal.snap
const CWD: &str = "
  Working directory · resume
  Session = latest cwd recorded in the resumed session

› 1. Use session directory (/Users/example/session)
  2. Use current directory (/Users/example/current)

  enter continue · esc use session · ⌃c quit";

// onboarding/auth.rs line 1361
const LOGIN: &str =
    "  Welcome to Codex\n\n> 1. Sign in with ChatGPT\n  2. Sign in with Device Code";

fn codex(steps: u32) -> CodexCli {
    CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    )
    .with_startup_poll(StartupPoll {
        step: Duration::from_millis(1),
        steps,
    })
}

/// Runs `answer_startup` on a screen that changes to `script[i].1` once
/// `script[i].0` keys were sent. Returns the outcome and every key sent.
fn run(
    first: &str,
    script: &[(usize, String)],
    policy: TrustPolicy,
    steps: u32,
) -> (Result<StartupOutcome, HarnessError>, Vec<Key>) {
    let host = FakeHost::new();
    let place: Place = host.add_place(
        "tw_demo_api",
        PlaceState::Blocked,
        Occupant {
            session: None,
            pids: vec![Pid::new(1)],
        },
    );
    host.set_screen(&place, first);
    let done = AtomicBool::new(false);
    let got = std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut shown = 0;
            while !done.load(Ordering::Relaxed) {
                let keys = host.keys_sent(&place).len();
                if let Some((i, (_, screen))) =
                    script.iter().enumerate().rfind(|(_, (n, _))| *n <= keys)
                    && i + 1 > shown
                {
                    host.set_screen(&place, screen);
                    shown = i + 1;
                }
                std::thread::sleep(Duration::from_micros(200));
            }
        });
        let got = codex(steps).answer_startup(&host, &place, policy);
        done.store(true, Ordering::Relaxed);
        got
    });
    (got, host.keys_sent(&place))
}

fn after(keys: usize, screen: &str) -> (usize, String) {
    (keys, screen.to_owned())
}

#[test]
fn a_composer_with_no_dialog_is_no_dialog() {
    let (got, keys) = run(IDLE, &[], TrustPolicy::Accept, 5);
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
    assert!(keys.is_empty());
}

#[test]
fn the_trust_dialog_is_accepted_with_enter_on_the_first_option() {
    let first = format!("{TRUST_TEXT}\n{TRUST}");
    let (got, keys) = run(&first, &[after(1, IDLE)], TrustPolicy::Accept, 200);
    assert_eq!(got.unwrap(), StartupOutcome::AnsweredPlainTrust);
    assert_eq!(keys, [Key::Enter]);
}

#[test]
fn the_pointer_is_moved_back_to_the_trusting_option_first() {
    let on_quit = format!("{TRUST_TEXT}\n{TRUST_ON_QUIT}");
    let on_trust = format!("{TRUST_TEXT}\n{TRUST}");
    let (got, keys) = run(
        &on_quit,
        &[after(1, &on_trust), after(2, IDLE)],
        TrustPolicy::Accept,
        200,
    );
    assert_eq!(got.unwrap(), StartupOutcome::AnsweredPlainTrust);
    assert_eq!(keys, [Key::Up, Key::Enter]);
}

#[test]
fn the_refuse_policy_types_nothing_into_the_trust_dialog() {
    let first = format!("{TRUST_TEXT}\n{TRUST}");
    let (got, keys) = run(&first, &[], TrustPolicy::Refuse, 5);
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty());
}

#[test]
fn the_hooks_review_is_never_answered() {
    for policy in [TrustPolicy::Accept, TrustPolicy::Refuse] {
        let (got, keys) = run(HOOKS_REVIEW, &[], policy, 5);
        assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
        assert!(keys.is_empty());
    }
}

#[test]
fn the_sign_in_screen_says_to_run_codex_login() {
    let (got, keys) = run(LOGIN, &[], TrustPolicy::Accept, 5);
    let err = got.unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Refused);
    assert!(err.detail.contains("codex login"), "{err}");
    assert!(keys.is_empty());
}

#[test]
fn the_update_prompt_is_skipped() {
    let one = format!("{UPDATE_HEAD}\n{UPDATE_1}");
    let two = format!("{UPDATE_HEAD}\n{UPDATE_2}");
    let (got, keys) = run(
        &one,
        &[after(1, &two), after(2, IDLE)],
        TrustPolicy::Accept,
        200,
    );
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
    assert_eq!(keys, [Key::Down, Key::Enter]);
}

#[test]
fn the_model_upgrade_is_refused_and_never_answered() {
    let one = format!("{MIGRATION_HEAD}\n{MIGRATION_1}");
    let (got, keys) = run(&one, &[], TrustPolicy::Accept, 5);
    let err = got.unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Refused);
    assert!(err.detail.contains("open Codex once"), "{err}");
    assert!(keys.is_empty());
}

#[test]
fn the_resume_folder_prompt_keeps_the_session_folder() {
    let (got, keys) = run(CWD, &[after(1, IDLE)], TrustPolicy::Accept, 200);
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
    assert_eq!(keys, [Key::Enter]);
}

#[test]
fn a_screen_that_never_settles_times_out() {
    let (got, _) = run("starting...", &[], TrustPolicy::Accept, 3);
    assert_eq!(got.unwrap_err().code, HarnessErrorCode::Timeout);
}
