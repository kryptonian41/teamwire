//! `screen_state` and `line_pending`, on text copied from the snapshot files
//! of Codex's TUI (`codex-rs/tui/src/**/snapshots/*.snap`, commit `8c07694`).

use std::sync::Arc;

use teamwire_core::{DialogKind, Harness, ScreenState, one_line};
use teamwire_fakes::FakePlatform;
use teamwire_harness_codex::{CodexCli, CodexConfig};

fn codex() -> CodexCli {
    CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    )
}

fn state(tail: &str) -> ScreenState {
    codex().screen_state(tail)
}

fn pending(tail: &str, text: &str) -> bool {
    codex().line_pending(tail, &one_line(text, 200))
}

// app/snapshots/codex_tui__app__owned_transcript__empty_state_animation_tests__fresh_thread_header.snap
const IDLE: &str = "
  >_ OpenAI Codex (v0.0.0)
     /tmp/project


› Ask Codex to do anything

  GPT-5.6-Sol default · /tmp/project
  ? for shortcuts                                                                              ⚠ 1 warning · f2 to view";

// bottom_pane/chat_composer/snapshots/*sparkle__tests__history_search_preserves_a_held_sparkle_command_without_reclassifying_it.snap
const IDLE_CONTEXT: &str = "
› Ask Codex to do anything

  ? for shortcuts                                            100% context left";

// app/snapshots/codex_tui__app__owned_transcript__follow_tests__running_follow_control.snap
const WORKING: &str = "transcript row 28

• Working (0s • esc to interrupt)

                             ↓ Back to bottom · esc

› draft stays here

  GPT-5.6-Sol default · /tmp/project
  tab to queue message";

// chatwidget/snapshots/codex_tui__chatwidget__tests__approval_modal_exec_no_reason.snap
const APPROVAL: &str = "

  Would you like to run the following command?

  $ echo hello world


› 1. Yes, proceed (y)
  2. Yes, and don't ask again for commands that start with `echo hello world` (p)
  3. No, and tell Codex what to do differently (esc)

  Press enter to confirm or esc to cancel";

// onboarding/snapshots/codex_tui__onboarding__trust_directory__tests__renders_snapshot_for_git_repo.snap
const TRUST: &str = "
  Folder access
  /workspace/project

  Trust this folder? Codex can read, edit, and run files here,
  subject to your permission settings. Folder settings can run code
  automatically, even without a model request. Continue only if you
  trust these files. Your trust decision will be saved.

› 1. Trust and continue
  2. Quit

  enter continue and create sandbox · esc quit";

// snapshots/codex_tui__startup_hooks_review__tests__startup_hooks_review_prompt.snap
const HOOKS_REVIEW: &str = "
  Hooks need review
  2 hooks are new or changed.
  Hooks can run outside the sandbox
  after you trust them.


› 1. Review hooks
  2. Trust all and continue
  3. Continue without trusting (hooks
     won't run)

  enter confirm · esc skip";

// snapshots/codex_tui__update_prompt__tests__update_prompt_modal.snap
const UPDATE: &str = "
  Update available · 0.0.0 → 9.9.9
  Release notes: https://github.com/openai/codex/releases/latest

› 1. Update now (runs `npm install -g @openai/codex`)
  2. Skip
  3. Skip until next version

  enter continue · esc skip";

// snapshots/codex_tui__cwd_prompt__tests__cwd_prompt_modal.snap
const CWD: &str = "
  Working directory · resume
  Session = latest cwd recorded in the resumed session
  Current = your current working directory

› 1. Use session directory (/Users/example/session)
  2. Use current directory (/Users/example/current)
  3. Always use session directory
  4. Always use current directory

  enter continue · esc use session · ⌃c quit";

// snapshots/codex_tui__model_migration__tests__model_migration_prompt.snap
const MIGRATION: &str = "
  Codex just got an upgrade. Introducing
  gpt-5.1-codex-max.

  Upgrade to gpt-5.2-codex for the latest and greatest
  agentic coding model.

› 1. Try new model
  2. Use existing model

  enter/esc confirm · ⌃c quit";

fn kind(tail: &str) -> DialogKind {
    codex().dialog_kind(tail)
}

// onboarding/auth.rs line 1361
const LOGIN: &str =
    "  Welcome to Codex\n\n> 1. Sign in with ChatGPT\n  2. Sign in with Device Code";

#[test]
fn an_approval_overlay_is_a_permission_dialog() {
    assert_eq!(kind(APPROVAL), DialogKind::Permission);
}

#[test]
fn the_trust_and_hooks_review_dialogs_are_trust() {
    assert_eq!(kind(TRUST), DialogKind::Trust);
    assert_eq!(kind(HOOKS_REVIEW), DialogKind::Trust);
}

#[test]
fn the_update_and_migration_dialogs_are_update() {
    assert_eq!(kind(UPDATE), DialogKind::Update);
    assert_eq!(kind(MIGRATION), DialogKind::Update);
}

#[test]
fn the_sign_in_dialog_is_login() {
    assert_eq!(kind(LOGIN), DialogKind::Login);
}

#[test]
fn the_resume_folder_picker_is_a_dialog_teamwire_cannot_name() {
    assert_eq!(kind(CWD), DialogKind::Unknown);
}

#[test]
fn the_idle_composer_and_a_working_turn_show_no_dialog() {
    assert_eq!(kind(IDLE), DialogKind::NoDialog);
    assert_eq!(kind(WORKING), DialogKind::NoDialog);
}

#[test]
fn the_idle_composer_with_its_footer_is_a_prompt() {
    assert_eq!(state(IDLE), ScreenState::InputPrompt);
    assert_eq!(state(IDLE_CONTEXT), ScreenState::InputPrompt);
}

#[test]
fn a_working_turn_is_not_a_prompt() {
    assert_ne!(state(WORKING), ScreenState::InputPrompt);
}

#[test]
fn each_dialog_is_a_dialog() {
    for (name, screen) in [
        ("approval", APPROVAL),
        ("trust", TRUST),
        ("hooks review", HOOKS_REVIEW),
        ("update", UPDATE),
        ("cwd", CWD),
        ("migration", MIGRATION),
    ] {
        assert_eq!(state(screen), ScreenState::Dialog, "{name}");
    }
}

#[test]
fn dialog_text_in_the_models_own_reply_is_not_a_dialog() {
    let screen = format!("• Update available means a new release exists.\n{IDLE_CONTEXT}");
    assert_eq!(state(&screen), ScreenState::InputPrompt);
}

#[test]
fn a_bare_pointer_without_a_footer_proves_nothing() {
    assert_eq!(state("ready\n› "), ScreenState::Unknown);
    assert_eq!(state(""), ScreenState::Unknown);
}

#[test]
fn a_line_in_the_composer_is_pending() {
    // The composer shows the typed line; the transcript has not echoed it.
    let tail = "• done\n\n› run the tests please\n\n  ? for shortcuts            97% context left";
    assert!(pending(tail, "run the tests please"));
    assert!(!pending(IDLE, "run the tests please"));
}

#[test]
fn a_line_already_sent_is_not_pending() {
    let tail = "› run the tests please\n\n• Working (1s • esc to interrupt)\n\n› \n\n  tab to queue message";
    assert!(!pending(tail, "run the tests please"));
}

#[test]
fn a_pasted_marker_in_the_composer_is_pending() {
    // app/snapshots/codex_tui__app__owned_transcript__input_tests__raw_paste_burst_render.snap
    let tail = "› [Pasted Content 2084 chars]\n\n  ? for shortcuts            97% context left";
    assert!(pending(tail, &"x".repeat(150)));
    assert!(!pending("› [Pasted Content many chars]", &"x".repeat(150)));
}
