//! `screen_state`: `InputPrompt` only on positive evidence.

use std::sync::Arc;

use teamwire_core::{Harness, ScreenState};
use teamwire_fakes::{
    ESCALATED_TRUST_TEXT, FakePlatform, PLAIN_TRUST_TEXT, PROMPT_TEXT, TRUST_OPTION_TEXT,
};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn state(tail: &str) -> ScreenState {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nowhere".into(),
            claude_json: "/nowhere/j".into(),
            claude_bin: "claude".into(),
        },
    )
    .screen_state(tail)
}

const BOX: &str =
    "╭──────────────────╮\n│ ❯                │\n╰──────────────────╯\n  ? for shortcuts";

#[test]
fn the_real_input_box_with_its_footer_is_a_prompt() {
    assert_eq!(state(BOX), ScreenState::InputPrompt);
    assert_eq!(
        state(
            "done.\n\n❯ Try \"fix the lint errors\"\n  ⏵⏵ bypass permissions on (shift+tab to cycle)"
        ),
        ScreenState::InputPrompt
    );
}

#[test]
fn a_bare_pointer_as_the_last_line_is_a_prompt() {
    assert_eq!(state("ready\n❯ "), ScreenState::InputPrompt);
    assert_eq!(
        state(&format!("ready\n{PROMPT_TEXT}")),
        ScreenState::InputPrompt
    );
}

#[test]
fn the_trust_dialog_is_a_dialog_although_its_pointer_looks_like_a_prompt() {
    let dialog = format!("{PLAIN_TRUST_TEXT}\n ❯ No, exit\n   {TRUST_OPTION_TEXT}");
    assert_eq!(state(&dialog), ScreenState::Dialog);
    assert_eq!(state(ESCALATED_TRUST_TEXT), ScreenState::Dialog);
}

#[test]
fn a_permission_prompt_is_a_dialog() {
    let screen = "Bash command\n  rm -rf x\n Do you want to proceed?\n ❯ 1. Yes\n   2. No";
    assert_eq!(state(screen), ScreenState::Dialog);
}

#[test]
fn a_dialog_scrolled_far_above_a_live_prompt_is_a_prompt() {
    // More than eight non-empty lines separate the old dialog from the box.
    let output = "output line
"
    .repeat(9);
    let screen = format!("{PLAIN_TRUST_TEXT}\n{TRUST_OPTION_TEXT}\n{output}{BOX}");
    assert_eq!(state(&screen), ScreenState::InputPrompt);
}

#[test]
fn a_permission_dialog_over_the_input_box_and_footer_is_a_dialog() {
    // Claude keeps the box and the footer on screen under a live dialog.
    let screen =
        format!("Bash command\n  rm -rf x\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n{BOX}");
    assert_eq!(state(&screen), ScreenState::Dialog);
}

#[test]
fn a_numbered_option_list_near_the_end_is_a_dialog_without_any_marker_text() {
    let screen = format!("Pick one\n ❯ 1. Yes\n   2. No\n{BOX}");
    assert_eq!(state(&screen), ScreenState::Dialog);
}

#[test]
fn each_dialog_marker_near_a_prompt_gives_dialog() {
    for marker in [
        TRUST_OPTION_TEXT,
        "Is this a project you created",
        "Do you want to proceed?",
        "Enter to confirm",
        "Esc to cancel",
        "These will apply without asking",
    ] {
        let screen = format!("{marker}\n{BOX}");
        assert_eq!(state(&screen), ScreenState::Dialog, "{marker}");
    }
}

#[test]
fn a_working_screen_with_no_prompt_is_unknown() {
    assert_eq!(state(""), ScreenState::Unknown);
    assert_eq!(
        state("* Thinking... (esc to interrupt)"),
        ScreenState::Unknown
    );
    assert_eq!(
        state("> quoted text in the middle\nmore output"),
        ScreenState::Unknown
    );
}

#[test]
fn a_numbered_list_in_the_last_reply_above_an_idle_box_is_a_prompt() {
    let screen = format!("⏺ Done. Summary:\n  1. Fixed the parser\n  2. Added a test\n\n{BOX}");
    assert_eq!(state(&screen), ScreenState::InputPrompt);
}

#[test]
fn marker_text_inside_a_reply_is_a_prompt() {
    let screen = format!("⏺ Press Esc to cancel a run, then Enter to confirm.\n\n{BOX}");
    assert_eq!(state(&screen), ScreenState::InputPrompt);
}
