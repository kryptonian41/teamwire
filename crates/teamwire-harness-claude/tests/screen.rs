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
fn a_dialog_scrolled_above_a_live_prompt_is_a_prompt() {
    let screen = format!("{PLAIN_TRUST_TEXT}\n{TRUST_OPTION_TEXT}\nwelcome\n{BOX}");
    assert_eq!(state(&screen), ScreenState::InputPrompt);
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
