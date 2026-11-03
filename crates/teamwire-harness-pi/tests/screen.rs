//! The screen rules against pi 1.4.0 screens captured from a tmux pane
//! (`tests/screens/`, plain and with style codes), and against screens
//! built from them for the states a capture could not show without a model
//! call: a working run, a large paste, and an extension's dialog.

use std::sync::Arc;

use teamwire_core::{Harness, ScreenState, one_line};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

const PROMPT: &str = include_str!("screens/prompt.txt");
const PROMPT_STYLED: &str = include_str!("screens/prompt_styled.txt");
const TYPED: &str = include_str!("screens/typed.txt");
const TYPED_STYLED: &str = include_str!("screens/typed_styled.txt");
const LONG: &str = include_str!("screens/long.txt");
const TRUST: &str = include_str!("screens/trust.txt");
const TRUST_STYLED: &str = include_str!("screens/trust_styled.txt");

/// The line typed into the box of `long.txt`.
fn long_line() -> String {
    let words = vec!["word"; 40].join(" ");
    format!("[teamwire:demo] from lead: {words} end")
}

fn pi() -> Pi {
    Pi::new(
        Arc::new(FakePlatform::new()),
        PiConfig {
            agent_dir: "/nowhere/pi".into(),
            pi_bin: "pi".into(),
            exclude_providers: Vec::new(),
        },
    )
}

const RULE: &str = "────────────────────────────────────────────────────────────";

/// The prompt screen with its top rule replaced by `top`.
fn with_top_rule(top: &str) -> String {
    let lines: Vec<&str> = PROMPT.lines().collect();
    let rules: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with('─'))
        .map(|(i, _)| i)
        .collect();
    let at = rules.iter().rev().nth(1).copied();
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| if Some(i) == at { top } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The prompt screen with `inside` between the box's rules.
fn with_box(inside: &str) -> String {
    format!(
        "{RULE}\n{inside}\n{RULE}\n/work (main)\n0.0%/1.0M (auto)      (anthropic) claude-sonnet • medium\n"
    )
}

#[test]
fn the_idle_screen_is_the_input_prompt_with_an_empty_box() {
    let pi = pi();
    for screen in [PROMPT, PROMPT_STYLED] {
        assert_eq!(pi.screen_state(screen), ScreenState::InputPrompt);
        assert!(!pi.box_holds_text(screen));
    }
}

#[test]
fn a_box_holding_typed_text_is_not_ready_for_a_line() {
    let pi = pi();
    for screen in [TYPED, TYPED_STYLED] {
        assert!(pi.box_holds_text(screen));
        assert_eq!(pi.screen_state(screen), ScreenState::Unknown);
    }
}

#[test]
fn the_trust_dialog_is_a_dialog() {
    let pi = pi();
    for screen in [TRUST, TRUST_STYLED] {
        assert_eq!(pi.screen_state(screen), ScreenState::Dialog);
        assert!(!pi.box_holds_text(screen));
    }
}

#[test]
fn a_working_run_is_not_the_input_prompt() {
    let busy = with_top_rule(&format!("── ⠋ Working (escape to interrupt) {RULE}"));
    assert_eq!(pi().screen_state(&busy), ScreenState::Unknown);
    let scrolled = with_top_rule(&format!("{RULE} ↑ 3 more {RULE}"));
    assert_eq!(pi().screen_state(&scrolled), ScreenState::InputPrompt);
}

#[test]
fn an_extension_dialog_in_the_box_is_a_dialog() {
    let dialog =
        with_box(" Allow tool call?\n → Yes\n   No\n ↑↓ navigate  enter select  escape cancel");
    assert_eq!(pi().screen_state(&dialog), ScreenState::Dialog);
    let input = with_box(" Your name\n > \n enter submit  escape cancel");
    assert_eq!(pi().screen_state(&input), ScreenState::Dialog);
}

#[test]
fn dialog_words_in_the_transcript_above_the_box_do_not_count() {
    let screen =
        format!(" The selector prints ↑↓ navigate  enter select  escape cancel.\n\n{PROMPT}");
    assert_eq!(pi().screen_state(&screen), ScreenState::InputPrompt);
}

#[test]
fn a_box_without_the_footer_is_unknown() {
    let screen = format!("{RULE}\n\n{RULE}\n");
    assert_eq!(pi().screen_state(&screen), ScreenState::Unknown);
    assert_eq!(pi().screen_state(""), ScreenState::Unknown);
}

#[test]
fn a_wrapped_line_still_in_the_box_is_pending() {
    let pi = pi();
    assert!(pi.line_pending(LONG, &one_line(&long_line(), 2000)));
    assert!(!pi.line_pending(LONG, &one_line("another line", 2000)));
    assert!(!pi.line_pending(PROMPT, &one_line(&long_line(), 2000)));
}

#[test]
fn a_line_wrapped_inside_a_word_is_still_pending() {
    let screen = with_box("[teamwire:demo] from lead: plea\nse rebase");
    assert!(pi().line_pending(
        &screen,
        &one_line("[teamwire:demo] from lead: please rebase", 200)
    ));
}

#[test]
fn a_paste_marker_counts_only_for_a_line_of_its_length() {
    let line = "x".repeat(1200);
    let screen = with_box("[paste #1 1200 chars]");
    assert!(pi().line_pending(&screen, &one_line(&line, 5000)));
    let other = with_box("[paste #1 1100 chars]");
    assert!(!pi().line_pending(&other, &one_line(&line, 5000)));
}
