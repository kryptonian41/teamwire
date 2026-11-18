//! Reading the Codex TUI's screen: is it waiting for input, or showing a
//! dialog? The answer decides whether a host may type into the session, so
//! `InputPrompt` needs positive evidence and nothing else is trusted.
//!
//! Every marker below is text from a snapshot file of Codex's TUI
//! (`tui/src/**/snapshots/*.snap`, commit `8c07694`).

pub(crate) use teamwire_core::screen::{has_any, squash};
use teamwire_core::{OneLine, ScreenState};

/// The folder-trust dialog (`onboarding/trust_directory.rs`).
pub(crate) const TRUST_MARKER: &str = "Trust this folder?";
/// The review of new or changed hooks (`startup_hooks_review.rs`). Its
/// second option trusts every hook, the user's own included.
pub(crate) const HOOKS_REVIEW_MARKER: &str = "Hooks need review";
pub(crate) const UPDATE_MARKER: &str = "Update available";
/// `model_migration.rs`.
pub(crate) const MIGRATION_MARKER: &str = "Codex just got an upgrade";
/// The resume picker that asks which folder to continue in (`cwd_prompt.rs`).
pub(crate) const CWD_MARKER: &str = "Working directory";

/// The question of each approval overlay: a tool-permission dialog.
pub(crate) const APPROVAL_MARKERS: [&str; 5] = [
    "Would you like to run the following command?",
    "Would you like to make the following edits?",
    "Would you like to grant these permissions?",
    "Allow this request?",
    "Do you want to approve network access",
];

/// Text a dialog or an approval overlay shows.
const DIALOG_MARKERS: [&str; 12] = [
    TRUST_MARKER,
    HOOKS_REVIEW_MARKER,
    UPDATE_MARKER,
    MIGRATION_MARKER,
    CWD_MARKER,
    APPROVAL_MARKERS[0],
    APPROVAL_MARKERS[1],
    APPROVAL_MARKERS[2],
    APPROVAL_MARKERS[3],
    APPROVAL_MARKERS[4],
    "Press enter to confirm or esc to cancel",
    "Press enter to submit or esc to cancel",
];

/// Footer text that only an idle composer prints.
const FOOTER_MARKERS: [&str; 2] = ["? for shortcuts", "context left"];

/// Footer or status text that only a working turn prints.
const BUSY_MARKER: &str = "esc to interrupt";

/// How many non-empty lines at the end of the screen can belong to the
/// composer, its footer, or a dialog drawn over them. A trust dialog on a
/// narrow terminal wraps to about 17 lines.
const BOX_LINES: usize = 20;

/// How much of a submitted line is looked for in the composer. The composer
/// wraps a long line, but its first row holds the start.
const PREFIX_CHARS: usize = 40;

/// The text after the pointer `›` (U+203A) that starts Codex's composer line
/// and marks the selected choice of a dialog.
pub(crate) fn pointer_rest(line: &str) -> Option<&str> {
    line.trim().strip_prefix('›').map(str::trim)
}

/// What the last pointer line of the screen says, such as `1. Trust and
/// continue`. A dialog redraws its pointer as the selection moves, and the
/// last one is the current selection.
pub(crate) fn last_pointer_text(screen: &str) -> Option<String> {
    screen.lines().rev().find_map(pointer_rest).map(squash)
}

/// A selected dialog choice, such as `› 1. Yes, proceed (y)`. The composer's
/// own pointer line never starts with a number and a dot.
pub(crate) fn is_selected_option(line: &str) -> bool {
    let Some(rest) = pointer_rest(line) else {
        return false;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest.get(digits..).is_some_and(|r| r.starts_with(". "))
}

/// A line of the model's reply or a tool header, which starts with `•`.
/// Marker text on such a line is something the model said, not a dialog.
fn is_transcript_line(line: &str) -> bool {
    line.trim_start().starts_with('•')
}

/// The non-empty lines at the end of the screen that can belong to the
/// composer, its footer, or a dialog.
pub(crate) fn last_lines(tail: &str) -> Vec<&str> {
    let lines: Vec<&str> = tail.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(BOX_LINES);
    lines.get(start..).unwrap_or_default().to_vec()
}

/// Whether a dialog is drawn at the end of the screen: marker text, or a
/// selected numbered choice, which a model's reply never prints.
pub(crate) fn dialog_near_the_end(last: &[&str]) -> bool {
    let drawn: Vec<&str> = last
        .iter()
        .copied()
        .filter(|l| !is_transcript_line(l))
        .collect();
    has_any(&squash(&drawn.join("\n")), &DIALOG_MARKERS)
        || drawn.iter().any(|l| is_selected_option(l))
}

fn at_input_prompt(last: &[&str]) -> bool {
    let busy = last.iter().any(|l| l.contains(BUSY_MARKER));
    !busy
        && last.iter().enumerate().any(|(i, line)| {
            pointer_rest(line).is_some()
                && last
                    .iter()
                    .skip(i + 1)
                    .any(|below| FOOTER_MARKERS.iter().any(|f| below.contains(f)))
        })
}

pub(crate) fn classify(tail: &str) -> ScreenState {
    let last = last_lines(tail);
    if dialog_near_the_end(&last) {
        ScreenState::Dialog
    } else if at_input_prompt(&last) {
        ScreenState::InputPrompt
    } else {
        ScreenState::Unknown
    }
}

/// Whether `line`, just submitted, still sits in the composer, unsent. Used
/// after a submit to decide on one more Enter.
pub(crate) fn line_pending(tail: &str, line: &OneLine) -> bool {
    let wanted: String = squash(line.as_str()).chars().take(PREFIX_CHARS).collect();
    // The last pointer line is the composer: a line echoed into the
    // transcript starts with `•` or `›`, and the composer's own pointer line
    // follows it.
    last_pointer_text(tail).is_some_and(|text| text.contains(&wanted) || is_paste_marker(&text))
}

/// A long paste shows in the composer as `[Pasted Content 2084 chars]` in
/// place of the text (`app/snapshots/*raw_paste_burst_render.snap`). Only a
/// composer holding nothing but such a marker counts.
// shortcut: any marker counts, not only one for the line teamwire pasted, so
// a person's own paste in the short window also gets the Enter. Upgrade by
// tying the marker's char count to the length of the pasted line.
fn is_paste_marker(text: &str) -> bool {
    text.strip_prefix("[Pasted Content ")
        .and_then(|rest| rest.strip_suffix(" chars]"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}
