//! `screen_state`: `InputPrompt` only on positive evidence.

use std::sync::Arc;

use teamwire_core::{
    Harness, Occupant, Pid, PlaceState, ScreenState, StartupOutcome, TrustPolicy, one_line,
};
use teamwire_fakes::{
    ESCALATED_TRUST_TEXT, FakeHost, FakePlatform, PLAIN_TRUST_TEXT, PROMPT_TEXT,
    TRUST_OPTION_TEXT,
};
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

fn state(tail: &str) -> ScreenState {
    claude().screen_state(tail)
}

fn pending(tail: &str, text: &str) -> bool {
    claude().line_pending(tail, &one_line(text, 200))
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

/// Claude Code's input box as it draws it today: a rule, the pointer line,
/// a rule, then the status line and the footer.
fn real_box(pointer: &str, footer: &str) -> String {
    let rule = "─".repeat(40);
    format!(
        "⏺ Done.\n\n{rule}\n{pointer}\n{rule}\n  💬 Teamwire team   │   Sonnet 5.5 · medium\n  {footer}"
    )
}

const IDLE_FOOTERS: [&str; 7] = [
    "⏵⏵ auto mode on · 1 monitor · ← 3 agents",
    "⏵⏵ auto mode on (shift+tab to cycle) · ← 3 agents",
    "⏵⏵ bypass permissions on · 2 monitors",
    "⏵⏵ accept edits on · 1 monitor",
    "⏸ plan mode on · ← 1 agent",
    "? for shortcuts",
    "← 3 agents",
];

#[test]
fn an_idle_box_is_a_prompt_whatever_the_footer_says() {
    for footer in IDLE_FOOTERS {
        assert_eq!(
            state(&real_box("❯", footer)),
            ScreenState::InputPrompt,
            "{footer}"
        );
    }
}

#[test]
fn delivery_waits_while_the_box_holds_text_and_the_trust_step_still_sees_a_prompt() {
    for footer in IDLE_FOOTERS {
        let screen = real_box("❯ fix the lint errors", footer);
        assert_eq!(state(&screen), ScreenState::Unknown, "{footer}");
        // The trust step reads the same box as the input prompt: no dialog.
        let host = FakeHost::new();
        let place = host.add_place(
            "tw_demo_api",
            PlaceState::Blocked,
            Occupant {
                session: None,
                pids: vec![Pid::new(1)],
            },
        );
        host.set_screen(&place, &screen);
        let got = claude().answer_startup(&host, &place, TrustPolicy::Accept);
        assert_eq!(got.unwrap(), StartupOutcome::NoDialog, "{footer}");
        assert!(host.keys_sent(&place).is_empty(), "{footer}");
    }
}

#[test]
fn a_busy_box_is_not_a_prompt() {
    let screen = real_box("❯", "⏵⏵ auto mode on · 1 monitor · esc to interrupt");
    assert_eq!(state(&screen), ScreenState::Unknown);
}

#[test]
fn a_dialog_over_a_ruled_box_is_a_dialog() {
    let rule = "─".repeat(40);
    let trust = format!(
        "{PLAIN_TRUST_TEXT}\n ❯ No, exit\n   {TRUST_OPTION_TEXT}\n{rule}\n  ⏵⏵ auto mode on · 1 monitor"
    );
    assert_eq!(state(&trust), ScreenState::Dialog);
    let permission = format!(
        "Do you want to proceed?\n ❯ 1. Yes\n   2. No\n{}",
        real_box("❯", "⏵⏵ auto mode on · 1 monitor")
    );
    assert_eq!(state(&permission), ScreenState::Dialog);
}

#[test]
fn a_pointer_line_without_a_ruled_box_or_footer_is_unknown() {
    assert_eq!(
        state("❯ No, exit\n  some text\n  more text"),
        ScreenState::Unknown
    );
}

#[test]
fn a_line_in_the_input_box_is_pending() {
    assert!(pending(
        "done\n❯ hello there\n  ? for shortcuts",
        "hello there"
    ));
    assert!(pending(
        "╭────╮\n│ > hello   there │\n╰────╯",
        "hello there"
    ));
}

#[test]
fn a_line_that_only_the_transcript_echoes_is_not_pending() {
    // The session took the line: its echo is followed by the box's own empty pointer.
    assert!(!pending("❯ hello there\n⏺ Working\n❯ ", "hello there"));
    assert!(!pending("ready\n❯ ", "hello there"));
    assert!(!pending("no pointer at all", "hello there"));
}

#[test]
fn only_the_first_forty_characters_are_matched() {
    let long = "word ".repeat(30);
    let first_row: String = long.chars().take(60).collect();
    assert!(pending(&format!("❯ {first_row}\n"), &long));
}

#[test]
fn a_box_holding_only_a_paste_marker_is_pending() {
    for marker in [
        "[Pasted text #1]",
        "[Pasted text #12 +34 lines]",
        "[Pasted text #2 +1 line]",
    ] {
        let tail = format!("done\n❯ {marker}\n  ? for shortcuts");
        assert!(
            pending(&tail, "a long line the box did not show"),
            "{marker}"
        );
    }
}

#[test]
fn text_that_only_mentions_a_paste_marker_is_not_pending() {
    assert!(!pending("❯ see [Pasted text #1] above\n", "something else"));
    assert!(!pending("❯ [Pasted text #]\n", "something else"));
    assert!(!pending("❯ [Pasted text #1 +x lines]\n", "something else"));
    assert!(!pending("[Pasted text #1]\n❯ \n", "something else"));
}
