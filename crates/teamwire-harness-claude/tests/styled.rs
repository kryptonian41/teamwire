//! Screens read with their style codes: dim placeholder text in the input
//! box is not typed text, and the codes never hide a prompt or a dialog.
//! The box lines are copied from Claude Code screens read with
//! `herdr agent read --format ansi`.

use std::sync::Arc;

use teamwire_core::{Harness, ScreenState};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn claude() -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nowhere".into(),
            claude_json: "/nowhere/j".into(),
            claude_bin: "claude".into(),
        },
    )
}

/// Claude Code's styled screen around `pointer`, the input box line.
fn styled_box(pointer: &str, footer: &str) -> String {
    let rule = format!("\u{1b}[38;2;136;136;136m{}\u{1b}[0m", "─".repeat(40));
    format!(
        "⏺ Done.\n\n{rule}\n{pointer}\n{rule}\n  \u{1b}[2m💬 Teamwire team\u{1b}[0m   │   Sonnet 5.5 · medium\n  {footer}"
    )
}

const IDLE: &str = "⏵⏵ auto mode on (shift+tab to cycle) · ← 3 agents";

/// A prompt suggestion after a turn, as Claude draws it.
const SUGGESTION: &str = "❯\u{a0}\u{1b}[0m\u{1b}[2mclose all three repro members\u{1b}[0m";

/// The hint an empty box shows while lines wait in the queue.
const QUEUED_HINT: &str = "\u{1b}[0m\u{1b}[38;2;153;153;153m❯\u{a0}\u{1b}[0m\u{1b}[2mPress up to edit queued messages\u{1b}[0m";

#[test]
fn a_prompt_suggestion_is_an_empty_box() {
    let screen = styled_box(SUGGESTION, IDLE);
    assert!(!claude().box_holds_text(&screen));
    assert_eq!(claude().screen_state(&screen), ScreenState::InputPrompt);
}

#[test]
fn the_queued_messages_hint_is_an_empty_box() {
    let footer = "⏵⏵ auto mode on · 1 shell · ← 3 agents";
    assert!(!claude().box_holds_text(&styled_box(QUEUED_HINT, footer)));
    // The same grey pointer before typed text: the box is found, and full.
    let typed = QUEUED_HINT.replace("\u{1b}[2mPress up to edit queued messages", "fix the lin");
    assert!(claude().box_holds_text(&styled_box(&typed, footer)));
}

#[test]
fn text_a_person_typed_is_text_on_a_styled_screen() {
    for typed in [
        "❯\u{a0}typed by a person",
        // A grey pointer: the `2` inside an RGB colour is not dim.
        "\u{1b}[38;2;153;153;153m❯\u{a0}\u{1b}[0mfix the lin",
        "\u{1b}[38;5;2m❯\u{a0}\u{1b}[0mfix the lin",
        // Dim switched off again before the typed text.
        "❯\u{a0}\u{1b}[2m\u{1b}[22mfix the lin",
        "❯\u{a0}\u{1b}[2m\u{1b}[mfix the lin",
    ] {
        let screen = styled_box(typed, IDLE);
        assert!(claude().box_holds_text(&screen), "{typed:?}");
        assert_eq!(
            claude().screen_state(&screen),
            ScreenState::Unknown,
            "{typed:?}"
        );
    }
}

#[test]
fn a_typed_line_after_a_suggestion_was_dismissed_is_text() {
    // Typing replaces the suggestion; text after dim text still counts.
    let screen = styled_box("❯\u{a0}\u{1b}[2mhint\u{1b}[0m and typed", IDLE);
    assert!(claude().box_holds_text(&screen));
}

#[test]
fn a_dim_dialog_hint_still_marks_a_dialog() {
    // The dim hint is the only dialog marker on this screen.
    let screen = format!(
        " \u{1b}[2mEsc to cancel\u{1b}[0m\n{}",
        styled_box("❯\u{a0}", IDLE)
    );
    assert_eq!(claude().screen_state(&screen), ScreenState::Dialog);
}

#[test]
fn a_hyperlink_code_is_not_text() {
    let link = "\u{1b}]8;;https://example.com\u{7}\u{1b}]8;;\u{1b}\\";
    let screen = styled_box(&format!("❯\u{a0}{link}"), IDLE);
    assert!(!claude().box_holds_text(&screen));
}
